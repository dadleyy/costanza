use async_std::channel;
use async_std::stream::StreamExt;
use std::io;

pub trait EffectCommandFilter {
  type Command;

  fn sendable(&self, command: &Self::Command) -> bool;
}

pub trait Effect {
  type Message;
  type Command;

  fn detach(&mut self) -> io::Result<(channel::Receiver<Self::Message>, channel::Sender<Self::Command>)>;
}

pub trait Application {
  type Message;
  type Command;

  fn update(self, message: Self::Message) -> (Self, Option<Vec<Self::Command>>)
  where
    Self: Sized;
}

pub struct EffectRuntime<M, C, A> {
  channels: Vec<(
    channel::Receiver<M>,
    channel::Sender<C>,
    Box<dyn EffectCommandFilter<Command = C>>,
  )>,
  application: A,
}

impl<M, C, A> EffectRuntime<M, C, A>
where
  A: Application<Message = M, Command = C>,
  M: std::fmt::Debug,
  C: std::fmt::Debug,
{
  pub fn new(a: A) -> Self {
    Self {
      application: a,
      channels: vec![],
    }
  }

  pub fn register<E, F>(&mut self, effect: &mut E, filter: F) -> io::Result<()>
  where
    E: Effect<Message = M, Command = C>,
    F: EffectCommandFilter<Command = C> + 'static,
  {
    let (s, r) = effect.detach()?;
    self.channels.push((s, r, Box::new(filter)));
    Ok(())
  }

  pub async fn run(self) -> io::Result<()> {
    let mut application = self.application;

    loop {
      let mut future_list = futures::stream::FuturesUnordered::new();

      for (message_receiver, _, _) in self.channels.iter() {
        future_list.push(message_receiver.recv());
      }

      let msg = match async_std::future::timeout(std::time::Duration::from_millis(100), future_list.next()).await {
        // No-op path
        Err(error) => {
          tracing::trace!("timeout on message channel receiving - {error:?}");
          continue;
        }

        // Unknown path
        Ok(None) => {
          tracing::trace!("empty message received from future unordered stream, maybe over?");
          continue;
        }

        // Sad path
        Ok(Some(Err(error))) => {
          tracing::error!("failed receive from a channel - {error}");
          break;
        }

        // Happy path
        Ok(Some(Ok(message))) => message,
      };

      tracing::debug!("applying message '{msg:?}' update to application");
      let (new_state, mut cmd) = application.update(msg);
      application = new_state;

      if let Some(command_list) = cmd.take() {
        for cmd in command_list {
          let serialized = format!("{cmd:?}");
          let mut sent = false;

          for (_, cmd_sink, filter) in self.channels.iter() {
            let sendable = filter.sendable(&cmd);
            tracing::debug!("checking senability of {serialized} ({sendable})");

            if !sendable {
              continue;
            }

            // Attempt to send the command.
            if let Err(error) = cmd_sink.send(cmd).await {
              tracing::warn!("failed sending command to sink - {error}");
              return Err(io::Error::new(io::ErrorKind::Other, "closed-sink"));
            }

            tracing::debug!("found home, sending command");
            sent = true;
            break;
          }

          if !sent {
            tracing::warn!("no channels able to process command {serialized}");
          }
        }
      }
    }

    Ok(())
  }
}
