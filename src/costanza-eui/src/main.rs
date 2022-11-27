use clap::Parser;
use iced::widget::{button, column};
use iced::{executor, Alignment, Application, Command, Element, Settings, Theme};

#[derive(Parser)]
struct CommandLineArguments {
  #[clap(short = 'c')]
  config: String,
}

pub fn main() -> iced::Result {
  if let Err(error) = dotenv::dotenv() {
    eprintln!("unable to load '.env' - {error}");
  }
  let config = CommandLineArguments::parse();

  let mut settings = Settings::with_flags(config);
  settings.window.size = (480, 272);
  settings.window.resizable = false;
  Counter::run(settings)
}

struct Counter {}

#[derive(Debug, Clone, Copy)]
enum Message {
  Home,
}

impl Application for Counter {
  type Message = Message;
  type Flags = CommandLineArguments;
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
