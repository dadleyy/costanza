//! This module contains the _most generic_ core of the architecture of this application - the
//! "glue" that, when given a bunch of different channels, and an application, will attempt to
//! continuously reach for the next command and "publish" it to the application.

use async_std::channel;
use async_std::stream::StreamExt;
use std::io;

/// The idea of this `EffectCommandFilter` is to be able to use a single `Command` type from the
/// application, but associate each effect manager with a filter that can be used to determine
/// whether or not the application command should apply to it.
///
/// A better generalization (I think) would be to have some kind of type guard on the generic
/// `Command` type parameter that can associate it with an instance of the channel somehow. That
/// part is not clear.
pub trait EffectCommandFilter {
  type Command;

  fn sendable(&self, command: &Self::Command) -> bool;
}

pub type UnbindResult<M, C> = io::Result<(channel::Receiver<M>, channel::Sender<C>)>;

pub trait Effect {
  type Message;
  type Command;

  fn detach(&mut self) -> UnbindResult<Self::Message, Self::Command>;
}

/// The main application trait. _Heavily_ inspired by the Elm architecture.
pub trait Application {
  type Message;
  type Command;
  type Flags;

  fn init(self, flags: Self::Flags) -> (Self, Option<Vec<Self::Command>>)
  where
    Self: Sized;

  fn update(self, message: Self::Message) -> (Self, Option<Vec<Self::Command>>)
  where
    Self: Sized;
}

/// Each effect runtime will provide us a pair of sender/receiver channels and a filter that we can
/// use to determine what commands belong to what channels.
struct EffectChannels<M, C>(
  channel::Receiver<M>,
  channel::Sender<C>,
  Box<dyn EffectCommandFilter<Command = C>>,
);

pub struct EffectRuntime<M, C, A, S>
where
  A: Application<Message = M, Command = C, Flags = S>,
{
  channels: Vec<EffectChannels<M, C>>,
  application: A,
}

impl<M, C, A, S> EffectRuntime<M, C, A, S>
where
  A: Application<Message = M, Command = C, Flags = S>,
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
    self.channels.push(EffectChannels(s, r, Box::new(filter)));
    Ok(())
  }

  pub async fn run(self, flags: S) -> io::Result<()> {
    let mut cursor = self.init(flags).await?;

    loop {
      cursor = match cursor.frame().await {
        Ok(next) => next,
        Err(error) => {
          tracing::error!("effect runtime terminal failure - {error}");
          break;
        }
      }
    }

    Ok(())
  }

  /// Wrapps the `Application` init with our command publishing.
  async fn init(self, flags: S) -> io::Result<Self> {
    let (application, mut cmds) = self.application.init(flags);

    let mut next = Self {
      application,
      channels: self.channels,
    };

    if let Some(command_list) = cmds.take() {
      next.publish_cmds(command_list).await?;
    }

    Ok(next)
  }

  #[inline]
  async fn frame(self) -> io::Result<Self> {
    // Create a list of futures that we will try to take the next ready and drop the rest. It is
    // not immediately clear right now if this can race or not (i.e: two futures ready at the same
    // time).
    let mut future_list = futures::stream::FuturesUnordered::new();
    let borrowed_iter = &self.channels;
    for EffectChannels(message_receiver, _, _) in borrowed_iter {
      future_list.push(message_receiver.recv());
    }
    let timeout_dur = std::time::Duration::from_millis(100);
    let message_result = async_std::future::timeout(timeout_dur, future_list.next()).await;
    drop(future_list);

    let msg = match message_result {
      // No-op path
      Err(error) => {
        tracing::trace!("timeout on message channel receiving - {error:?}");
        return Ok(Self {
          application: self.application,
          channels: self.channels,
        });
      }

      // Unknown path
      Ok(None) => {
        tracing::trace!("empty message received from future unordered stream, maybe over?");
        return Ok(Self {
          application: self.application,
          channels: self.channels,
        });
      }

      // Sad path
      Ok(Some(Err(error))) => {
        tracing::error!("failed receive from a channel - {error}");
        return Err(io::Error::new(io::ErrorKind::Other, format!("{error}")));
      }

      // Happy path
      Ok(Some(Ok(message))) => message,
    };

    tracing::debug!("applying message '{msg:?}' update to application");
    let (new_state, mut cmd) = self.application.update(msg);

    let mut next = Self {
      application: new_state,
      channels: self.channels,
    };

    if let Some(command_list) = cmd.take() {
      next.publish_cmds(command_list).await?;
    }

    Ok(next)
  }

  /// Given a mutable borrow to an instance of this runtime and a vector of commands to publish,
  /// this function will attempt to iterate over them and figure out who to send to and how to send
  /// them.
  async fn publish_cmds(&mut self, command_list: Vec<C>) -> io::Result<()> {
    for cmd in command_list {
      #[cfg(debug_assertions)]
      let serialized = format!("{cmd:?}");

      #[cfg(debug_assertions)]
      let mut sent = false;

      for EffectChannels(_, cmd_sink, filter) in self.channels.iter() {
        let sendable = filter.sendable(&cmd);

        #[cfg(debug_assertions)]
        tracing::debug!("checking senability of {serialized} ({sendable})");

        if !sendable {
          continue;
        }

        // Attempt to send the command.
        if let Err(error) = cmd_sink.send(cmd).await {
          tracing::warn!("failed sending command to sink - {error}");
          return Err(io::Error::new(io::ErrorKind::Other, "closed-sink"));
        }

        #[cfg(debug_assertions)]
        {
          sent = true;
        }

        break;
      }

      #[cfg(debug_assertions)]
      if !sent {
        tracing::warn!("no channels able to process command {serialized}");
      }
    }

    Ok(())
  }
}
