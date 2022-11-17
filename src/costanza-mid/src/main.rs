use clap::Parser;
use futures_lite::future::FutureExt;
use futures_lite::StreamExt;
use serde::Deserialize;
use std::io;

#[derive(Deserialize, Debug)]
struct SerialConfiguration {
  device: String,
  baud: u32,
}

#[derive(Deserialize, Debug)]
struct Configuration {
  http: costanza::server::Configuration,
  serial: Option<SerialConfiguration>,
}

#[derive(Parser)]
struct CommandLineArguments {
  #[clap(long, short)]
  config: String,
}

async fn serial(mut config: Option<SerialConfiguration>) -> io::Result<()> {
  let span = tracing::span!(tracing::Level::INFO, "serial");
  let _ = span.enter();

  tracing::info!("serial task working off {config:?}");

  let mut interval = async_std::stream::interval(std::time::Duration::from_millis(5000));

  // As we go about our day, we may or may not actually have a serial port available.
  let mut port = None;

  let mut failures = 0;

  // A hack for working around our initial interval poll.
  let mut should_wait = false;

  loop {
    if should_wait {
      interval.next().await;
    } else {
      should_wait = true;
    }

    tracing::event!(parent: &span, tracing::Level::INFO, "serial frame");

    // Always start with a reconnect attempt if it is necessary.
    (config, port) = match (config.take(), port.take()) {
      (Some(config), None) => {
        tracing::event!(
          parent: &span,
          tracing::Level::INFO,
          "no active port, attempting to connect"
        );

        // Attempt the connection
        let serial = match serialport::new(&config.device, config.baud).open() {
          Err(error) => {
            tracing::warn!("unable to open serial port - {error}");
            failures += 1;
            None
          }
          Ok(port) => {
            tracing::info!("fresh connection established via {config:?}");
            failures = 0;
            Some(port)
          }
        };

        (Some(config), serial)
      }
      (config, Some(port)) => (config, Some(port)),
      (_, _) => (None, None),
    };

    // The remainder of each iteration here is only concerned with active ports.
    if port.is_none() {
      continue;
    }

    if let Some(mut active_port) = port.as_mut() {
      tracing::info!("has port, attempting write");

      if let Err(error) = io::Write::write(&mut active_port, b"hello\n") {
        tracing::warn!("failed write - {error}");
        port.take();
        continue;
      }
    }
  }

  Ok(())
}

async fn effects() -> io::Result<()> {
  let span = tracing::span!(tracing::Level::INFO, "effects");
  let _ = span.enter();

  let mut failures = 0;
  let mut interval = async_std::stream::interval(std::time::Duration::from_millis(5000));

  loop {
    interval.next().await;
    tracing::event!(parent: &span, tracing::Level::INFO, "effect frame");

    failures += 1;
    if failures > 100 {
      break;
    }
  }

  Ok(())
}

async fn run(config: Configuration) -> io::Result<()> {
  serial(config.serial)
    .race(costanza::server::start(config.http))
    .race(effects())
    .await
}

fn main() -> io::Result<()> {
  let arguments = CommandLineArguments::parse();
  let config_contents = std::fs::read_to_string(&arguments.config)?;
  let config = toml::from_str::<Configuration>(config_contents.as_str())?;

  tracing_subscriber::fmt().init();
  tracing::event!(tracing::Level::INFO, "configuration ready, running application");
  tracing::event!(tracing::Level::DEBUG, "{config:?}");
  async_std::task::block_on(run(config))
}
