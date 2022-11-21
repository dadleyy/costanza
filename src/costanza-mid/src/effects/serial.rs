use async_std::channel;
use serde::Deserialize;
use std::io;

#[derive(Deserialize, Debug)]
pub struct SerialConfiguration {
  device: String,
  baud: u32,
}

pub trait OuputParser {
  type Message;

  fn parse(&self, data: &[u8]) -> Option<(Self::Message, usize)>;
}

pub struct Serial<C, M, O> {
  parser: O,
  commands: (channel::Receiver<C>, Option<channel::Sender<C>>),
  messages: (channel::Sender<M>, Option<channel::Receiver<M>>),
  buffer: Vec<u8>,
  config: Option<SerialConfiguration>,
  config_channel: (
    channel::Sender<Option<SerialConfiguration>>,
    channel::Receiver<Option<SerialConfiguration>>,
  ),
}

impl<C, M, O> Serial<C, M, O>
where
  C: std::fmt::Debug + std::fmt::Display,
  O: OuputParser<Message = M>,
{
  pub fn new(config: Option<SerialConfiguration>, parser: O) -> Self {
    let commands = channel::unbounded();
    let messages = channel::unbounded();
    let config_channel = channel::unbounded();

    Self {
      parser,
      config,
      buffer: vec![],
      config_channel,
      commands: (commands.1, Some(commands.0)),
      messages: (messages.0, Some(messages.1)),
    }
  }

  pub fn config_channel(&self) -> channel::Sender<Option<SerialConfiguration>> {
    self.config_channel.0.clone()
  }

  pub async fn run(mut self) -> io::Result<()> {
    let mut port = None;

    loop {
      match self.config_channel.1.try_recv() {
        Ok(config) => self.config = config,
        Err(error) if error.is_empty() => (),
        Err(error) => {
          break Err(io::Error::new(
            io::ErrorKind::Other,
            format!("closed serial config channel - {error}"),
          ))
        }
      }

      port = match (self.config.as_ref(), port.take()) {
        (Some(config), None) => {
          let new_port = serialport::new(&config.device, config.baud)
            .open()
            .map_err(|error| {
              tracing::warn!("unable to open port - {error}");
              error
            })
            .ok();

          if new_port.is_some() {
            tracing::info!("established new connection to our serial port");
          }

          new_port
        }
        (_, Some(port)) => Some(port),
        (None, None) => None,
      };

      if port.is_none() {
        async_std::task::sleep(std::time::Duration::from_secs(2)).await;
        continue;
      }

      // Attempt to read from the serial port.
      let mut unwrapped_port = port.as_mut().unwrap();
      let mut buffer = [0u8; 1024];
      match io::Read::read(&mut unwrapped_port, &mut buffer) {
        Err(error) if error.kind() == io::ErrorKind::TimedOut => (),

        Err(error) => {
          tracing::warn!("unable to read from port - {error}");
          port = None;
          async_std::task::sleep(std::time::Duration::from_secs(2)).await;
          continue;
        }

        Ok(amount) => self.buffer.extend_from_slice(&buffer[0..amount]),
      }

      // If we have content in our buffer, attempt to parse it and truncate the buffer back down to
      // the amount of bytes the message consumes.
      if !self.buffer.is_empty() {
        if let Some((message, bytes_taken)) = self.parser.parse(&self.buffer) {
          if let Err(error) = self.messages.0.send(message).await {
            tracing::warn!("unable to propagate parsed message - {error}");
            break Err(io::Error::new(io::ErrorKind::Other, "failed-serial-message-send"));
          }

          self.buffer = self.buffer.into_iter().skip(bytes_taken).collect();
          tracing::debug!("current buffer after truncate - {:X?}", self.buffer);
        }
      }

      // Check to see if we have anything waiting to be sent into our serial port.
      match self.commands.0.try_recv() {
        Err(error) if error.is_empty() => (),
        Err(error) => {
          let message = format!("closed serial command channel ({error})");
          break Err(io::Error::new(io::ErrorKind::Other, message));
        }
        Ok(command) => {
          tracing::info!("serial command ready to send - {command:?}");

          if let Err(error) = io::Write::write(&mut unwrapped_port, format!("{command}").as_bytes()) {
            tracing::warn!("failed writing of command - {error}");
            break Err(io::Error::new(io::ErrorKind::Other, error));
          }
        }
      }

      // Sleep for a little bit to yield to other tasks.
      async_std::task::sleep(std::time::Duration::from_millis(50)).await;
    }
  }
}

impl<C, M, O> crate::eff::Effect for Serial<C, M, O> {
  type Message = M;
  type Command = C;

  fn detach(&mut self) -> io::Result<(channel::Receiver<Self::Message>, channel::Sender<Self::Command>)> {
    let cmd_in = self
      .commands
      .1
      .take()
      .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "already taken"))?;

    let msg_out = self
      .messages
      .1
      .take()
      .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "already taken"))?;

    Ok((msg_out, cmd_in))
  }
}
