use async_std::channel;
use async_std::stream::StreamExt;
use std::io;

pub struct Ticker<C, M> {
  interval: std::time::Duration,
  commands: (channel::Receiver<C>, Option<channel::Sender<C>>),
  messages: (channel::Sender<M>, Option<channel::Receiver<M>>),
}

impl<C, M> Ticker<C, M>
where
  M: std::fmt::Debug,
{
  pub fn new(interval: std::time::Duration) -> Self {
    let commands = channel::unbounded();
    let messages = channel::unbounded();

    Self {
      interval,
      commands: (commands.1, Some(commands.0)),
      messages: (messages.0, Some(messages.1)),
    }
  }

  pub async fn run<F>(self, f: F) -> io::Result<()>
  where
    F: Fn() -> M,
  {
    let mut ival = async_std::stream::interval(self.interval);

    loop {
      ival.next().await;
      let message = f();
      tracing::debug!("sending ticker message - {message:?}");

      if let Err(error) = self.messages.0.send(message).await {
        tracing::warn!("unable to send tick - {error}");
        break;
      }
    }

    Err(io::Error::new(io::ErrorKind::Other, "closing ticker effect channel"))
  }
}

impl<C, M> crate::eff::Effect for Ticker<C, M> {
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
