//! The serial side effect wraps an underlying tty-looking serial connection. The effect manager
//! will attempt to use a `SerialCommandMap` to both:
//!
//! 1. Create application-specific messages for connections and disconnect events.
//! 2. Map an application-specific command into the generic command type defined here.

use async_std::channel;
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Deserialize, Debug, Serialize, Clone)]
pub struct SerialConfiguration {
  device: String,
  baud: u32,
}

/// The output parser is the type that is used to produce the application-specific messages _from_
/// serial data.
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
}

/// The `SerialCommand` type defined here refers to types that are uniquely related to the serial
/// effect management; they are more specific than the general application config.
pub enum SerialCommand<D>
where
  D: std::fmt::Display,
{
  Control(bool),
  Configure(SerialConfiguration),
  Data(D),
}

pub trait SerialCommandMap<D>
where
  D: std::fmt::Display,
{
  type Command;
  type Message;

  fn translate(&self, original: Self::Command) -> Option<SerialCommand<D>>;

  /// Defines the type of message that should be used when we lose a serial connection.
  fn disconnected(&self) -> Self::Message;

  /// Defines the type of message that should be used when we establish a serial connection.
  fn connected(&self) -> Self::Message;
}

impl<C, M, O> Serial<C, M, O>
where
  O: OuputParser<Message = M>,
{
  pub fn new(config: Option<SerialConfiguration>, parser: O) -> Self {
    let commands = channel::unbounded();
    let messages = channel::unbounded();

    Self {
      parser,
      config,
      buffer: vec![],
      commands: (commands.1, Some(commands.0)),
      messages: (messages.0, Some(messages.1)),
    }
  }

  pub async fn run<T, D>(mut self, glue: T) -> io::Result<()>
  where
    T: SerialCommandMap<D, Command = C, Message = M>,
    D: std::fmt::Display,
  {
    let mut port = None;
    let mut is_connected = false;
    let mut manual_disconnect = false;

    loop {
      // Check to see if we have anything waiting to be sent into our serial port, or if we have a
      // configuration command that can be extrapolated from the original command.
      let sendable_command = match self.commands.0.try_recv() {
        Err(error) if error.is_empty() => None,
        Err(error) => {
          let message = format!("closed serial command channel ({error})");
          break Err(io::Error::new(io::ErrorKind::Other, message));
        }
        Ok(command) => match glue.translate(command) {
          // When a user has explictly sent a control command, we'll use the `manual_disconnect`
          // flag to circumvent any attempt to connect.
          Some(SerialCommand::Control(true)) => {
            manual_disconnect = false;
            None
          }
          Some(SerialCommand::Control(false)) => {
            manual_disconnect = true;
            port = None;
            None
          }

          Some(SerialCommand::Configure(config)) => {
            self.config = Some(config);
            None
          }
          Some(SerialCommand::Data(serializable)) => Some(format!("{serializable}")),
          None => {
            tracing::warn!("unable to map from external serial command to internal command");
            None
          }
        },
      };

      port = match (manual_disconnect, self.config.as_ref(), port.take()) {
        (true, _, _) => None,
        (_, Some(config), None) => {
          let new_port = serialport::new(&config.device, config.baud)
            .open()
            .map_err(|error| {
              tracing::warn!("unable to open {:?} port - {error}", config);
              error
            })
            .ok();

          if new_port.is_some() {
            tracing::info!("established new connection to our serial port");

            if !is_connected {
              is_connected = true;

              self.messages.0.send(glue.connected()).await.map_err(|error| {
                tracing::warn!("unable to send connected message - {error}");
                io::Error::new(io::ErrorKind::Other, format!("serial-send failure: {error}"))
              })?;
            }
          }

          new_port
        }
        (_, _, Some(port)) => Some(port),
        (_, None, None) => None,
      };

      if port.is_none() {
        // If we were connected and are no longer, ask our map to create a message that can be
        // used to notify the application we have disconnected.
        if is_connected {
          is_connected = false;

          self.messages.0.send(glue.disconnected()).await.map_err(|error| {
            tracing::warn!("unable to send disconnect message - {error}");
            io::Error::new(io::ErrorKind::Other, format!("serial-send failure: {error}"))
          })?;
        }

        // If we received a command and were able to get something that implements the `Display`
        // trait (was serializable), we have "dropped" a message that would've otherwise been sent.
        if let Some(dropped) = sendable_command {
          tracing::warn!("dropping received command due to missing serial connection - {dropped}");
        }

        async_std::task::sleep(std::time::Duration::from_secs(2)).await;
        continue;
      }

      // Attempt to read from the serial port.
      let mut unwrapped_port = port.as_mut().unwrap();
      let mut buffer = [0u8; 1024];
      match io::Read::read(&mut unwrapped_port, &mut buffer) {
        // TODO: do we need to consider timeouts here?
        Err(error) if error.kind() == io::ErrorKind::TimedOut => (),

        Err(error) => {
          tracing::warn!("unable to read from port - {error}");
          // Clear out the current port and sleep for a bit. Our next loop will be responsible for
          // the reconnection attempt.
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
          // Attempt to send our now-parsed message to the effect runtime.
          if let Err(error) = self.messages.0.send(message).await {
            tracing::warn!("unable to propagate parsed message - {error}");
            break Err(io::Error::new(io::ErrorKind::Other, "failed-serial-message-send"));
          }

          self.buffer = self.buffer.into_iter().skip(bytes_taken).collect();
          tracing::debug!("current buffer after truncate - {:X?}", self.buffer);
        }
      }

      // If, at the start of this iteration, we had a command we should be able to publish it now.
      // If that fails, we will clear out the connection.
      if let Some(payload) = sendable_command {
        if let Err(error) = write!(unwrapped_port, "{payload}") {
          tracing::warn!("unable to write command - {error}");
          port = None;
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
