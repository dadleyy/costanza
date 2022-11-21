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
  http: costanza::effects::http::Configuration,

  /// The configuration used by the serial connection.
  serial: Option<costanza::effects::serial::SerialConfiguration>,
}

#[derive(Parser)]
#[clap(version = option_env!("COSTANZA_VERSION").unwrap_or("dev"))]
struct CommandLineArguments {
  #[clap(long, short)]
  config: String,
}

#[derive(Debug)]
enum Message {
  Tick,
  Serial(String),
  Http(costanza::effects::http::Message),

  DisconnectedSerial,
  ConnectedSerial,
}

#[derive(Debug, PartialEq, Eq)]
enum SerialCommand {
  #[allow(dead_code)]
  Raw(String),
}

#[derive(Deserialize, Serialize, Debug)]
struct RawSerialRequest {
  tick: u32,
  value: String,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ClientMessage {
  RawSerial(RawSerialRequest),
}

impl std::fmt::Display for SerialCommand {
  fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
    match &self {
      SerialCommand::Raw(inner) => writeln!(formatter, "{inner}"),
    }
  }
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
  #[allow(dead_code)]
  Serial(SerialCommand),

  Http(costanza::effects::http::Command),
}

impl std::fmt::Display for Command {
  fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
    match self {
      Command::Serial(inner) => write!(formatter, "{inner}"),
      Command::Http(_) => Ok(()),
    }
  }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct ReceivedDataEntry {
  content: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "history_kind", rename_all = "snake_case")]
enum ClientHistoryEntry {
  SentCommand(ClientMessage),
  ReceivedData(ReceivedDataEntry),
}

#[derive(Serialize, Debug, Default)]
struct DerivedClientState {
  tick: u32,
  history: Vec<ClientHistoryEntry>,

  /// Whether or not the serial connection is available.
  serial_available: bool,
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
struct Application {
  last_home: Option<std::time::Instant>,
  connected_clients: std::collections::HashMap<String, DerivedClientState>,
  serial_available: bool,
}

impl costanza::eff::Application for Application {
  type Message = Message;
  type Command = Command;

  fn update(self, message: Self::Message) -> (Self, Option<Vec<Self::Command>>) {
    let mut next = self;

    match message {
      kind @ Message::DisconnectedSerial | kind @ Message::ConnectedSerial => {
        let serial_available = matches!(kind, Message::ConnectedSerial);

        // Store the state on the application state itself. This will be used as new clients
        // connect so they have a fresh connection value without having to rely on these messages
        // being received.
        next.serial_available = serial_available;

        // If we have no clients to also update, we're done.
        if next.connected_clients.is_empty() {
          return (next, None);
        }

        let mut cmds = vec![];
        for (id, client) in &mut next.connected_clients {
          client.serial_available = serial_available;

          match serde_json::to_string(&ResponseKinds::State(client)) {
            Ok(payload) => {
              cmds.push(Command::Http(costanza::effects::http::Command::SendState(
                id.clone(),
                payload,
              )));
            }
            Err(error) => {
              tracing::warn!("uanble to serialize client state - {error}");
            }
          }
        }
        return (next, Some(cmds));
      }

      Message::Http(costanza::effects::http::Message::ClientDisconnected(id)) => {
        tracing::info!("client {id} disconnected");
        next.connected_clients.remove(&id);
      }

      // When a client sends us data, we receive it as a raw string and are left to determine what
      // to do with it ourselves.
      Message::Http(costanza::effects::http::Message::ClientData(id, data)) => {
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

          // Update the "tick" that we're using based on the message provided
          tracing::info!("has parsed client data - {parsed:?}");
          client.tick = match &parsed {
            ClientMessage::RawSerial(inner) => {
              cmds.push(Command::Serial(SerialCommand::Raw(inner.value.clone())));
              inner.tick
            }
          };

          // Add this interaction to our history
          client.history.push(ClientHistoryEntry::SentCommand(parsed));

          // Create the response that we'll send back to the client.
          let response = &ResponseKinds::Response(ClientResponse {
            tick: client.tick,
            status: "ok".into(),
          });

          // Immediately return a command that will let our client know we have received their
          // request.
          match serde_json::to_string(&response) {
            Ok(res) => {
              cmds.push(Command::Http(costanza::effects::http::Command::SendState(
                id.clone(),
                res,
              )));
              return (next, Some(cmds));
            }
            Err(error) => tracing::warn!("unable to serialize - {error}"),
          }
        }

        tracing::warn!("unable to fiend client id {id}");
      }

      // When clients connect, create an entry for them.
      Message::Http(costanza::effects::http::Message::ClientConnected(id)) => {
        tracing::info!("has new client, updating hash");
        // Populate this new client with the latest connection state available to us.
        let connected_client = DerivedClientState {
          serial_available: next.serial_available,
          ..DerivedClientState::default()
        };

        next.connected_clients.insert(id, connected_client);
      }

      Message::Serial(data) => {
        tracing::info!("has serial data - {data}");

        if !next.connected_clients.is_empty() {
          let mut cmds = vec![];

          // Add this serial message to all of our connected clients.
          for (id, client) in &mut next.connected_clients {
            client.history.push(ClientHistoryEntry::ReceivedData(ReceivedDataEntry {
              content: data.clone(),
            }));

            match serde_json::to_string(&ResponseKinds::State(client)) {
              Ok(payload) => {
                let response_command = Command::Http(costanza::effects::http::Command::SendState(id.clone(), payload));
                cmds.push(response_command);
              }
              Err(error) => tracing::warn!("unable to serialize payload - {error}"),
            }
          }

          return (next, Some(cmds));
        }
      }

      Message::Tick => {
        if next.last_home.is_none() {
          next.last_home = Some(std::time::Instant::now());
          return (next, None);
        }

        let last_home = next.last_home.unwrap();
        let now = std::time::Instant::now();

        if now.duration_since(last_home).as_secs() < 2 {
          return (next, None);
        }

        next.last_home = Some(now);

        if next.connected_clients.is_empty() {
          return (next, None);
        }

        tracing::info!("has {} clients to send heartbeats to", next.connected_clients.len());
        let mut cmds = vec![];
        for (id, client) in &next.connected_clients {
          match serde_json::to_string(&ResponseKinds::State(client)) {
            Ok(payload) => {
              let response_command = Command::Http(costanza::effects::http::Command::SendState(id.clone(), payload));
              cmds.push(response_command);
            }
            Err(error) => tracing::warn!("unable to serialize client tick response - {error}"),
          }
        }

        return (next, Some(cmds));
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
    .race(s.run(|| Message::ConnectedSerial, || Message::DisconnectedSerial))
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
