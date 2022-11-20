use async_std::channel;
use async_std::prelude::FutureExt;
use serde::Serialize;
use std::io;

pub struct Http<C, M> {
  config: crate::server::Configuration,
  commands: (channel::Receiver<C>, Option<channel::Sender<C>>),
  messages: (channel::Sender<M>, Option<channel::Receiver<M>>),
}

impl<C, M> Http<C, M>
where
  M: std::fmt::Debug,
{
  pub fn new(config: crate::server::Configuration) -> Self {
    let commands = channel::unbounded();
    let messages = channel::unbounded();

    Self {
      config,
      commands: (commands.1, Some(commands.0)),
      messages: (messages.0, Some(messages.1)),
    }
  }

  pub async fn run<CM, MM>(self, cm: CM, mm: MM) -> io::Result<()>
  where
    CM: Fn(C) -> Option<crate::server::Command>,
    MM: Fn(crate::server::Message) -> M,
  {
    let out = channel::unbounded();
    let inp = channel::unbounded();
    let runtime = crate::server::ServerRuntime::new(self.config, (out.0.clone(), inp.1));

    async_std::task::spawn(async move { runtime.run().await });

    // Our main "thread" here will be concerned with pulling messages from what is sent from the
    // runtime and passing it through to the effect runtime.
    loop {
      let any_closed =
        self.commands.0.is_closed() || self.messages.0.is_closed() || inp.0.is_closed() || out.1.is_closed();

      if any_closed {
        tracing::warn!("detected http channel closure, terminating http effect manager thread");
        return Ok(());
      }

      let command_handler = async {
        // Attempt to see if we have a command to send into.
        let command = self
          .commands
          .0
          .recv()
          .await
          .map_err(|error| io::Error::new(io::ErrorKind::Other, format!("failed http command proxy - {error}")))?;

        if let Some(inner) = cm(command) {
          inp
            .0
            .send(inner)
            .await
            .map_err(|error| io::Error::new(io::ErrorKind::Other, format!("failed http command proxy - {error}")))?;
        }

        Ok(()) as io::Result<()>
      };

      let message_handler = async {
        // Attempt to see if we have a message from the runtime to send out.
        let message = out
          .1
          .recv()
          .await
          .map_err(|error| io::Error::new(io::ErrorKind::Other, format!("failed http message receive - {error}")))?;
        self
          .messages
          .0
          .send(mm(message))
          .await
          .map_err(|error| io::Error::new(io::ErrorKind::Other, format!("failed http message receive - {error}")))?;
        Ok(()) as io::Result<()>
      };

      command_handler.race(message_handler).await?;
    }
  }
}

impl<C, M> crate::eff::Effect for Http<C, M> {
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
