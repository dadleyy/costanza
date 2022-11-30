#![forbid(unsafe_code)]

use clap::Parser;
use std::io;
use tracing_subscriber::prelude::*;

#[derive(Parser)]
#[clap(version = option_env!("COSTANZA_VERSION").unwrap_or("dev"))]
struct CommandLineArguments {
  #[clap(long, short)]
  config: String,
}

fn main() -> io::Result<()> {
  if let Err(error) = dotenv::dotenv() {
    eprintln!("no '.env' file found ({error})");
  }
  let arguments = CommandLineArguments::parse();
  let config_contents = std::fs::read_to_string(&arguments.config)?;
  let config = toml::from_str::<costanza::Configuration>(config_contents.as_str())?;

  tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer())
    .with(tracing_subscriber::EnvFilter::from_default_env())
    .init();

  tracing::event!(tracing::Level::INFO, "configuration ready, running application");
  tracing::event!(tracing::Level::DEBUG, "{config:?}");
  async_std::task::block_on(costanza::run(config))
}
