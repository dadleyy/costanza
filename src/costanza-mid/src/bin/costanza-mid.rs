#![forbid(unsafe_code)]

use clap::Parser;
use futures_lite::future::FutureExt;
use serde::{Deserialize, Serialize};
use std::io;

/// The configuration we will load from the filesystem is an amalgamation of internal
/// configurations for the various effect systems.
#[derive(Deserialize, Debug)]
struct Configuration {
  /// The configuration used by our http server.
  http: costanza::server::Configuration,

  /// The configuration used by the serial connection.
  serial: Option<costanza::effects::serial::SerialConfiguration>,
}

#[derive(Parser)]
struct CommandLineArguments {
  #[clap(long, short)]
  config: String,
}

#[derive(Debug)]
enum Message {
  Tick,
  Serial(String),
  Http(costanza::server::Message),
}

#[derive(Debug, PartialEq, Eq)]
enum SerialCommand {
  #[allow(dead_code)]
  Ping,

  Raw(String),
}

#[derive(Deserialize, Debug)]
struct RawSerialRequest {
  tick: u32,
  value: String,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "kind")]
#[serde(rename_all = "snake_case")]
enum ClientMessage {
  RawSerial(RawSerialRequest),
}

impl std::fmt::Display for SerialCommand {
  fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
    match &self {
      SerialCommand::Ping => writeln!(formatter, "hi"),
      SerialCommand::Raw(inner) => writeln!(formatter, "{inner}"),
    }
  }
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
  #[allow(dead_code)]
  Serial(SerialCommand),

  Http(costanza::server::Command),
}

impl std::fmt::Display for Command {
  fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
    match self {
      Command::Serial(inner) => write!(formatter, "{inner}"),
      Command::Http(_) => Ok(()),
    }
  }
}

#[derive(Serialize, Debug, Default)]
struct DerivedClientState {
  tick: u32,
}

#[derive(Serialize, Debug, Default)]
struct ClientResponse {
  tick: u32,
  status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
enum ResponseKinds<'a> {
  State(&'a DerivedClientState),
  Response(ClientResponse),
}

#[derive(Default)]
struct ClientState {
  history: Vec<ClientMessage>,
  derived: DerivedClientState,
}

#[derive(Default)]
struct Application {
  last_home: Option<std::time::Instant>,
  connected_clients: std::collections::HashMap<String, ClientState>,
}

impl costanza::eff::Application for Application {
  type Message = Message;
  type Command = Command;

  fn update(self, message: Self::Message) -> (Self, Option<Vec<Self::Command>>) {
    let mut next = self;
    tracing::info!("application message received - {message:?}");

    match message {
      Message::Http(costanza::server::Message::ClientDisconnected(id)) => {
        tracing::info!("client {id} disconnected");
        next.connected_clients.remove(&id);
      }

      // When a client sends us data, we receive it as a raw string and are left to determine what
      // to do with it ourselves.
      Message::Http(costanza::server::Message::ClientData(id, data)) => {
        tracing::info!("handling client '{id}' data '{data}'");

        let parsed = match serde_json::from_str::<ClientMessage>(&data) {
          Err(error) => {
            tracing::warn!("unable to parse client data - {error}");
            return (next, None);
          }
          Ok(p) => p,
        };

        if let Some(client) = next.connected_clients.get_mut(&id) {
          let mut cmds = vec![];

          tracing::info!("has parsed client data - {parsed:?}");
          client.derived.tick = match &parsed {
            ClientMessage::RawSerial(inner) => {
              cmds.push(Command::Serial(SerialCommand::Raw(inner.value.clone())));
              inner.tick
            }
          };
          client.history.push(parsed);

          // Immediately return a command that will let our client know we have received their
          // request.
          if let Ok(res) = serde_json::to_string(&ResponseKinds::Response(ClientResponse {
            tick: client.derived.tick,
            status: "ok".into(),
          })) {
            cmds.push(Command::Http(costanza::server::Command::SendState(id.clone(), res)));
            return (next, Some(cmds));
          }
        }

        tracing::warn!("unable to fiend client id {id}");
      }

      // When clients connect, create an entry for them.
      Message::Http(costanza::server::Message::ClientConnected(id)) => {
        tracing::info!("has new client, updating hash");
        next.connected_clients.insert(id, ClientState::default());
      }
      _ => (),
    }

    if next.last_home.is_none() {
      next.last_home = Some(std::time::Instant::now());
      return (next, None);
    }

    if let Some(last_home) = next.last_home {
      let now = std::time::Instant::now();

      if now.duration_since(last_home).as_secs() > 2 {
        next.last_home = Some(now);

        if !next.connected_clients.is_empty() {
          tracing::info!("has {} clients to send heartbeats to", next.connected_clients.len());

          let mut cmds = vec![];
          for (id, client) in &next.connected_clients {
            if let Ok(payload) = serde_json::to_string(&ResponseKinds::State(&client.derived)) {
              cmds.push(Command::Http(costanza::server::Command::SendState(id.clone(), payload)));
            }
          }

          return (next, Some(cmds));
        }

        return (next, None);
      }
    }

    (next, None)
  }
}

struct SerialFilter {}
impl costanza::eff::EffectCommandFilter for SerialFilter {
  type Command = Command;
  fn sendable(&self, c: &Self::Command) -> bool {
    matches!(c, Command::Serial(_))
  }
}

struct SerialParser {}
impl costanza::effects::serial::OuputParser for SerialParser {
  type Message = Message;

  fn parse(&self, bytes: &[u8]) -> Option<(Self::Message, usize)> {
    let appearance = String::from_utf8_lossy(bytes);

    if let Some(boundary) = appearance.find('\n') {
      if !appearance.is_char_boundary(boundary) {
        tracing::warn!("newline appeared at strange location in utf8 byte array, ignoring");
        return None;
      }

      let full = appearance.chars().take(boundary).collect::<String>();
      return Some((Message::Serial(full), boundary + 1));
    }

    None
  }
}

struct TickFilter {}
impl costanza::eff::EffectCommandFilter for TickFilter {
  type Command = Command;
  fn sendable(&self, _: &Self::Command) -> bool {
    false
  }
}

struct HttpFilter {}
impl costanza::eff::EffectCommandFilter for HttpFilter {
  type Command = Command;

  fn sendable(&self, command: &Self::Command) -> bool {
    matches!(command, Command::Http(_))
  }
}

async fn run(config: Configuration) -> io::Result<()> {
  // Create all of our effect managers
  let mut s = costanza::effects::serial::Serial::new(None, SerialParser {});
  let mut t = costanza::effects::ticker::Ticker::new(std::time::Duration::from_secs(1));
  let mut h = costanza::effects::http::Http::new(config.http);

  s.config_channel()
    .send(config.serial)
    .await
    .expect("unable to populate initial serial configuration");

  // Create the main effect runtime using a default application state
  let mut runtime = costanza::eff::EffectRuntime::new(Application::default());

  // Register the side effect managers
  runtime.register(&mut s, SerialFilter {})?;
  runtime.register(&mut t, TickFilter {})?;
  runtime.register(&mut h, HttpFilter {})?;

  // Run all.
  runtime
    .run()
    .race(s.run())
    .race(t.run(|| Message::Tick))
    .race(h.run(
      |c| match c {
        Command::Http(inner) => Some(inner),
        _ => None,
      },
      Message::Http,
    ))
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
