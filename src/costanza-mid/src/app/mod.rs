#![forbid(unsafe_code)]

use crate::effects;
use futures_lite::future::FutureExt;
use serde::{Deserialize, Serialize};
use std::io;

/// The timing configuration is used to hold all of the application-specific timing requirements
/// that we may have, including the websocket broadcast interval and our tick time itself.
#[derive(Deserialize, Debug, Clone)]
struct TimingConfiguration {
  broadcast_interval: u64,
}

/// The configuration we will load from the filesystem is an amalgamation of internal
/// configurations for the various effect systems.
#[derive(Deserialize, Debug, Clone)]
pub struct Configuration {
  /// The configuration used by our http server.
  http: effects::http::Configuration,

  /// The configuration used by the serial connection.
  serial: Option<effects::serial::SerialConfiguration>,

  timing: Option<TimingConfiguration>,
}

#[derive(Debug)]
enum Message {
  /// The `Tick` message is used to process serial events.
  Tick,

  /// The `Broadcast` message is used to publish websocket events to clients.
  Broadcast,

  Serial(String),
  Http(effects::http::Message),

  DisconnectedSerial,
  ConnectedSerial,
}

#[derive(Debug)]
enum SerialCommand {
  #[allow(dead_code)]
  Raw(String),

  Status,

  Configure(effects::serial::SerialConfiguration),
}

impl std::fmt::Display for SerialCommand {
  fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
    match &self {
      SerialCommand::Raw(inner) => writeln!(formatter, "{inner}"),
      SerialCommand::Status => writeln!(formatter, "?"),
      _ => Ok(()),
    }
  }
}

#[derive(Deserialize, Serialize, Debug)]
struct RawSerialRequest {
  tick: u32,
  value: String,
}

/// This type represents the schema of data that can be sent from individual websocket
/// connections. The `Application` receives that data as raw `String` data and will attempt to
/// parse it here as json.
#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ClientMessage {
  RawSerial(RawSerialRequest),
  Configuration(effects::serial::SerialConfiguration),
}

#[derive(Debug)]
enum Command {
  #[allow(dead_code)]
  Serial(SerialCommand),

  Http(effects::http::Command),
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

#[derive(Debug, Default)]
enum SerialConnectionState {
  #[default]
  Disconnected,

  Idle(Option<std::time::Instant>),

  PendingAttempt,

  SendingFile(String),
}

impl SerialConnectionState {
  fn available(&self) -> bool {
    matches!(&self, SerialConnectionState::Idle(_))
  }
}

#[derive(Default)]
struct Application {
  /// The `last_broadcast` field is used to determine during which tick we should broadcast all
  /// updated state messages to connected clients.
  last_broadcast: Option<std::time::Instant>,

  /// The map of connected clients available to us through websockets.
  connected_clients: std::collections::HashMap<String, DerivedClientState>,

  /// Whether or not our serial connection is available.
  serial: SerialConnectionState,
}

impl Application {
  /// There are a few times where we will want to append to a list of commands a "state refresh"
  /// command for every client that is connected:
  ///
  /// 1. disconnect
  /// 2. connect
  /// 3. pending connect
  /// 4. etc...
  fn add_statuses(&mut self, command_list: &mut Vec<Command>) {
    for (id, client) in &mut self.connected_clients {
      client.serial_available = self.serial.available();

      match serde_json::to_string(&ResponseKinds::State(client)) {
        Ok(payload) => {
          command_list.push(Command::Http(effects::http::Command::SendState(id.clone(), payload)));
        }
        Err(error) => {
          tracing::warn!("uanble to serialize client state - {error}");
        }
      }
    }
  }
}

impl crate::eff::Application for Application {
  type Message = Message;
  type Command = Command;
  type Flags = Configuration;

  fn init(self, flags: Self::Flags) -> (Self, Option<Vec<Self::Command>>) {
    if let Some(config) = flags.serial {
      let config_cmd = Command::Serial(SerialCommand::Configure(config));
      tracing::info!("sending initial serial configuration");
      return (self, Some(vec![config_cmd]));
    }

    (self, None)
  }

  fn update(self, message: Self::Message) -> (Self, Option<Vec<Self::Command>>) {
    let mut next = self;

    match message {
      kind @ Message::DisconnectedSerial | kind @ Message::ConnectedSerial => {
        let serial_available = matches!(kind, Message::ConnectedSerial);

        // Store the state on the application state itself. This will be used as new clients
        // connect so they have a fresh connection value without having to rely on these messages
        // being received.
        next.serial = if serial_available {
          tracing::info!("serial connection available + idle");
          SerialConnectionState::Idle(None)
        } else {
          tracing::warn!("serial connection disconnect");
          SerialConnectionState::Disconnected
        };

        // If we have no clients to also update, we're done.
        if next.connected_clients.is_empty() {
          return (next, None);
        }

        let mut cmds = vec![];
        next.add_statuses(&mut cmds);
        return (next, Some(cmds));
      }

      Message::Http(effects::http::Message::FileUpload(file_contents)) => {
        if !next.serial.available() {
          tracing::warn!("was not ready to handle a file upload");
          return (next, None);
        }

        tracing::info!("has uploaded file ({file_contents:?})");
        next.serial = SerialConnectionState::SendingFile(file_contents);
        return (next, None);
      }

      Message::Http(effects::http::Message::ClientDisconnected(id)) => {
        tracing::debug!("client {id} disconnected");
        next.connected_clients.remove(&id);
      }

      // When a client sends us data, we receive it as a raw string and are left to determine what
      // to do with it ourselves.
      Message::Http(effects::http::Message::ClientData(id, data)) => {
        tracing::debug!("handling client '{id}' data '{data}'");

        let parsed = match serde_json::from_str::<ClientMessage>(&data) {
          Err(error) => {
            tracing::warn!("unable to parse client data - {error}");
            return (next, None);
          }
          Ok(p) => p,
        };

        let maybe_client = next.connected_clients.get_mut(&id);

        if maybe_client.is_none() {
          return (next, None);
        }

        // Now that we have proven this is a valid request, we know we're going to be creating some
        // commands and we can unwrap the `Option`.
        let mut connected_client = maybe_client.unwrap();
        let mut cmds = vec![];

        // Update the "tick" that we're using based on the message provided
        tracing::debug!("has parsed client data - {parsed:?}");

        connected_client.tick = match &parsed {
          ClientMessage::Configuration(configuration) => {
            // Create an attempt to configure our serial connection and make note of it on our
            // internal, mutable state.
            cmds.push(Command::Serial(SerialCommand::Configure(configuration.clone())));
            next.serial = SerialConnectionState::PendingAttempt;

            connected_client.tick
          }

          ClientMessage::RawSerial(inner) => {
            cmds.push(Command::Serial(SerialCommand::Raw(inner.value.clone())));
            let tick = inner.tick;

            // Add this interaction to our history
            connected_client.history.push(ClientHistoryEntry::SentCommand(parsed));

            tick
          }
        };

        // Create the response that we'll send back to the client.
        let response = &ResponseKinds::Response(ClientResponse {
          tick: connected_client.tick,
          status: "ok".into(),
        });

        // Immediately return a command that will let our client know we have received their
        // request.
        match serde_json::to_string(&response) {
          Ok(res) => {
            cmds.push(Command::Http(effects::http::Command::SendState(id.clone(), res)));
          }
          Err(error) => tracing::warn!("unable to serialize - {error}"),
        }

        // Now, we _also_ want to send along a fresh set of state updates since we know we're about
        // to be disconnecting from, and attempting to connect to a new serial device.
        next.add_statuses(&mut cmds);

        tracing::warn!("unable to fiend client id {id}");
        return (next, Some(cmds));
      }

      // When clients connect, create an entry for them.
      Message::Http(effects::http::Message::ClientConnected(id)) => {
        tracing::debug!("has new client, updating hash");
        // Populate this new client with the latest connection state available to us.
        let connected_client = DerivedClientState {
          serial_available: next.serial.available(),
          ..DerivedClientState::default()
        };

        next.connected_clients.insert(id, connected_client);
      }

      Message::Serial(data) => {
        tracing::debug!("has serial data - {data}");

        if !next.connected_clients.is_empty() {
          let mut cmds = vec![];

          // Add this serial message to all of our connected clients.
          for (id, client) in &mut next.connected_clients {
            client.history.push(ClientHistoryEntry::ReceivedData(ReceivedDataEntry {
              content: data.clone(),
            }));

            match serde_json::to_string(&ResponseKinds::State(client)) {
              Ok(payload) => {
                let response_command = Command::Http(effects::http::Command::SendState(id.clone(), payload));
                cmds.push(response_command);
              }
              Err(error) => tracing::warn!("unable to serialize payload - {error}"),
            }
          }

          return (next, Some(cmds));
        }
      }

      Message::Broadcast => {
        // If we have never broadcast before, just update our reference and send anything we have.
        if next.last_broadcast.is_none() {
          next.last_broadcast = Some(std::time::Instant::now());
          return (next, None);
        }

        let last_broadcast = next.last_broadcast.unwrap();
        let now = std::time::Instant::now();

        // If we've broadcasted to our clients recently, just skip.
        if now.duration_since(last_broadcast).as_secs() < 2 {
          return (next, None);
        }

        next.last_broadcast = Some(now);

        // We don't need to continue if we have no connected clients.
        if next.connected_clients.is_empty() {
          return (next, None);
        }

        tracing::debug!("has {} clients to send heartbeats to", next.connected_clients.len());
        let mut cmds = Vec::with_capacity(10);
        next.add_statuses(&mut cmds);
        return (next, Some(cmds));
      }

      Message::Tick => {
        let mut cmds = vec![];

        // Start by seeing if we are sending a file over. If so, we will attempt to take the next
        // line off the contents and push a raw serial cmd onto our return vector.
        if let SerialConnectionState::SendingFile(contents) = next.serial {
          let mut lines = contents.lines();
          let next_line = lines.next();

          if let Some(next_line) = next_line {
            // We have a line, grab the contents and create a raw serial command for it.
            tracing::info!("sending next file line '{next_line:?}'");
            cmds.push(Command::Serial(SerialCommand::Raw(next_line.to_string())));

            // TODO: our lines iterator trims the newline off the rest of our lines. There is
            // probably a way to do this so we hold into the original iterator instead of
            // manipulating it back and forth between iterator and concrete string.
            let remainder = lines.map(|l| format!("{l}\n")).collect::<String>();

            next.serial = SerialConnectionState::SendingFile(remainder);
          } else {
            next.serial = SerialConnectionState::Idle(None);
          }
        }

        if let SerialConnectionState::Idle(last_ping) = next.serial {
          let now = std::time::Instant::now();
          let mut is_old = last_ping.is_none();

          if let Some(ping) = last_ping {
            is_old = now.duration_since(ping).as_secs() > 3;
          }

          if is_old {
            tracing::info!("sending new ping to serial");
            next.serial = SerialConnectionState::Idle(Some(now));
            cmds.push(Command::Serial(SerialCommand::Status));
          }
        }

        return (next, Some(cmds));
      }
    }

    (next, None)
  }
}

struct SerialFilter {}
impl crate::eff::EffectCommandFilter for SerialFilter {
  type Command = Command;
  fn sendable(&self, c: &Self::Command) -> bool {
    matches!(c, Command::Serial(_))
  }
}

struct SerialParser {}
impl effects::serial::OuputParser for SerialParser {
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
impl crate::eff::EffectCommandFilter for TickFilter {
  type Command = Command;
  fn sendable(&self, _: &Self::Command) -> bool {
    false
  }
}

struct HttpFilter {}
impl crate::eff::EffectCommandFilter for HttpFilter {
  type Command = Command;

  fn sendable(&self, command: &Self::Command) -> bool {
    matches!(command, Command::Http(_))
  }
}

struct SerialMap {}
impl effects::serial::SerialCommandMap<SerialCommand> for SerialMap {
  type Command = Command;
  type Message = Message;

  fn translate(&self, original: Self::Command) -> Option<effects::serial::SerialCommand<SerialCommand>> {
    let serial_command = match original {
      Command::Serial(inner) => inner,
      _ => return None,
    };

    Some(match serial_command {
      SerialCommand::Configure(config) => effects::serial::SerialCommand::Configure(config),
      SerialCommand::Raw(data) => effects::serial::SerialCommand::Data(SerialCommand::Raw(data)),
      SerialCommand::Status => effects::serial::SerialCommand::Data(SerialCommand::Status),
    })
  }

  fn disconnected(&self) -> Self::Message {
    Message::DisconnectedSerial
  }

  fn connected(&self) -> Self::Message {
    Message::ConnectedSerial
  }
}

pub async fn run(config: Configuration) -> io::Result<()> {
  // Create all of our effect managers
  let mut serial_effects = effects::serial::Serial::new(None, SerialParser {});
  let mut http_effects = effects::http::Http::new(config.http.clone());

  // The serial ticks are actually the maxiumum frequency that _we_ will be sending commands to the
  // serial connection. The `serial_effects` manager is responsible for inbound traffic from the
  // connection.
  let mut serial_ticks = effects::ticker::Ticker::new(std::time::Duration::from_millis(50));

  // Create the ticker that will be used to create event which we can use to determine when to
  // publish events to our websockets.
  let broadcast_interval = config.timing.as_ref().map(|t| t.broadcast_interval).unwrap_or(2);
  tracing::info!("configured using broadcast interval - {broadcast_interval}s");
  let mut broadcast_ticks = effects::ticker::Ticker::new(std::time::Duration::from_secs(broadcast_interval));

  // Create the main effect runtime using a default application state
  let mut runtime = crate::eff::EffectRuntime::new(Application::default());

  // Register the side effect managers
  runtime.register(&mut serial_ticks, TickFilter {})?;
  runtime.register(&mut broadcast_ticks, TickFilter {})?;

  runtime.register(&mut serial_effects, SerialFilter {})?;
  runtime.register(&mut http_effects, HttpFilter {})?;

  // Run all.
  runtime
    .run(config.clone())
    // Separate message creation for tick/broadcst.
    .race(broadcast_ticks.run(|| Message::Broadcast))
    .race(serial_ticks.run(|| Message::Tick))
    // Provide the unique application events for connection and disconnection.
    .race(serial_effects.run(SerialMap {}))
    .race(http_effects.run(
      |c| match c {
        Command::Http(inner) => Some(inner),
        _ => None,
      },
      Message::Http,
    ))
    .await
}
