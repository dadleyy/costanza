#![allow(dead_code, unused)]

//! TODO: this whole application is pending active construction. What exists here only serves to
//! provide a distributable artifact for hardware/os verification purposes.

use clap::Parser;
use iced::widget::{button, column};
use iced::{executor, Alignment, Application, Command, Element, Settings, Theme};
use serde::Deserialize;
use std::io;

#[derive(Deserialize)]
struct WebsocketConfiguration {
  addr: String,
}

#[derive(Deserialize)]
struct Configuration {
  websocket: WebsocketConfiguration,
}

#[derive(Parser)]
struct CommandLineArguments {
  #[clap(short = 'c')]
  config: String,
}

pub fn main() -> io::Result<()> {
  if let Err(error) = dotenv::dotenv() {
    eprintln!("unable to load '.env' - {error}");
  }
  let args = CommandLineArguments::parse();
  let contents = std::fs::read_to_string(&args.config)?;
  let config = toml::from_str::<Configuration>(&contents)?;
  let mut settings = Settings::with_flags(config);
  settings.window.size = (480, 272);
  settings.window.resizable = false;
  Counter::run(settings).map_err(|error| io::Error::new(io::ErrorKind::Other, format!("runtime error - {error}")))
}

struct Counter {}

#[derive(Debug, Clone, Copy)]
enum Message {
  Home,
}

impl Application for Counter {
  type Message = Message;
  type Flags = Configuration;
  type Executor = executor::Default;
  type Theme = Theme;

  fn new(flags: Self::Flags) -> (Self, Command<Message>) {
    (Self {}, Command::none())
  }

  fn title(&self) -> String {
    String::from("Counter - Iced")
  }

  fn update(&mut self, message: Message) -> Command<Message> {
    Command::none()
  }

  fn view(&self) -> Element<Message> {
    column![button("Home").on_press(Message::Home),]
      .padding(20)
      .align_items(Alignment::Center)
      .into()
  }
}
