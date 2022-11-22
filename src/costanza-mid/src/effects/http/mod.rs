#![warn(clippy::missing_docs_in_private_items)]

//! This module contains the types related to side effects associated with http connections to our
//! server.

use async_std::{channel, sync};
use futures_lite::{FutureExt, StreamExt};
use serde::Serialize;
use std::io;

/// The `auth_routes` module defines the routes responsible for authenticating users.
mod auth_routes;

/// The `file_routes` deals with uploading files.
mod file_routes;

/// Contains configuration structure.
mod configuration;

/// Cookie and other compile-time constants.
mod constants;

/// Types related to Auth0 (current recommended oauth provider)
mod oauth;

/// Cookie + JWT related types.
mod sec;

/// The shared "request runtime" types.
mod shared_state;

/// General utility functionality.
mod utils;

pub use configuration::Configuration;

/// The command type here represents effects that a concrete `eff::Application` can send into our
/// web runtime.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Command {
  /// When the concrete application runtime needs to send a payload to a connected websocket,
  /// this command will be returned which contains the id of a client and the payload to send.
  SendState(String, String),
}

/// The message type here are the possible messages produced by this effect runtime that are
/// consumed by the concrete application runtime.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Message {
  /// A message that will be sent to the concrete application runtime containing a client id.
  ClientConnected(String),

  /// A message that will be sent to the concrete application runtime containing a client id and
  /// any data that was received by that client.
  ClientData(String, String),

  /// When a file is uploaded, we will...
  FileUpload(String),

  /// A message that will be sent to the concrete application runtime containing a client id.
  ClientDisconnected(String),
}

/// The `Http` effect  is responsible for creating a server runtime and passing message/command
/// values pulled off its channels along.
pub struct Http<C, M> {
  /// The top-level http effect runtime configuration.
  config: Configuration,

  /// The channel pair used to pull commands from the application runtime.
  commands: (channel::Receiver<C>, Option<channel::Sender<C>>),

  /// The channel pair used to send messages to the application runtime.
  messages: (channel::Sender<M>, Option<channel::Receiver<M>>),
}

impl<C, M> Http<C, M>
where
  M: std::fmt::Debug,
{
  /// Return a new http effect manager based on a provided configuration.
  pub fn new(config: Configuration) -> Self {
    let commands = channel::unbounded();
    let messages = channel::unbounded();

    Self {
      config,
      commands: (commands.1, Some(commands.0)),
      messages: (messages.0, Some(messages.1)),
    }
  }

  /// This is the main entrypoint to the http effect runtime. It is reponsible for spawning the
  /// server runtime and:
  ///
  /// 1. allowing consumers to map commands returned by the application runtime into the internal
  ///    command type provided by this module.
  /// 2. allowing consumers to map messages from the internal runtime here into their own
  ///    application message domain.
  pub async fn run<CM, MM>(self, command_mapper: CM, message_mapper: MM) -> io::Result<()>
  where
    CM: Fn(C) -> Option<Command>,
    MM: Fn(Message) -> M,
  {
    // Create some proxy channels that will be used to send commands and messages between this
    // runtime and the underlying server runtime.
    let message_proxy = channel::unbounded();
    let command_proxy = channel::unbounded();

    // Create the underlying server runtime and execute in in a separate task.
    let runtime = ServerRuntime::new(self.config, (message_proxy.0.clone(), command_proxy.1));
    async_std::task::spawn(async move { runtime.run().await });

    // Our main "thread" here will be concerned with pulling messages from what is sent from the
    // runtime and passing it through to the effect runtime.
    loop {
      let any_closed = self.commands.0.is_closed()
        || self.messages.0.is_closed()
        || command_proxy.0.is_closed()
        || message_proxy.1.is_closed();

      if any_closed {
        tracing::warn!("detected http channel closure, terminating http effect manager thread");
        return Ok(());
      }

      // The command half our proxy will attempt to pull commands from the effect runtime channel
      // and pass them along to the command channel created and provided to the server runtime.
      let command_handler = async {
        // Attempt to see if we have a command to send into.
        let command = self
          .commands
          .0
          .recv()
          .await
          .map_err(|error| io::Error::new(io::ErrorKind::Other, format!("failed http command proxy - {error}")))?;

        // The consumer has a chance to return a `None` based on the command, similar to the filter
        // function provided to the generic types provided by the `eff` module.
        if let Some(inner) = command_mapper(command) {
          command_proxy
            .0
            .send(inner)
            .await
            .map_err(|error| io::Error::new(io::ErrorKind::Other, format!("failed http command proxy - {error}")))?;
        }

        Ok(()) as io::Result<()>
      };

      // The message half of our proxy will attempt to pull messages from the channel provided to
      // the server runtime and pass them along back out to our main effect runtime channel.
      let message_handler = async {
        // Attempt to see if we have a message from the runtime to send out.
        let message = message_proxy
          .1
          .recv()
          .await
          .map_err(|error| io::Error::new(io::ErrorKind::Other, format!("failed http message receive - {error}")))?;
        self
          .messages
          .0
          .send(message_mapper(message))
          .await
          .map_err(|error| io::Error::new(io::ErrorKind::Other, format!("failed http message receive - {error}")))?;
        Ok(()) as io::Result<()>
      };

      if let Err(error) = command_handler.race(message_handler).await {
        tracing::warn!("http effect runtime channel proxy failure - {error}");
        break;
      }
    }

    Err(io::Error::new(io::ErrorKind::Other, "http-runtime failure"))
  }
}

impl<C, M> crate::eff::Effect for Http<C, M> {
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

/// The schema of our basic, heartbeat message.
#[derive(Serialize)]
struct Heartbeat {
  /// The current time of our server.
  time: std::time::SystemTime,
}

/// route: returns the system time. can be used as a health check endpoint.
async fn heartbeat(request: tide::Request<shared_state::SharedState>) -> tide::Result {
  let shared_state::SharedState {
    span,
    config: _,
    redis: _,
    messages: _,
    registration: _,
  } = request.state();
  let span = tracing::span!(parent: span, tracing::Level::INFO, "heartbeat");
  tracing::event!(parent: &span, tracing::Level::INFO, "returning basic status info");
  tide::Body::from_json(&Heartbeat {
    time: std::time::SystemTime::now(),
  })
  .map(|body| tide::Response::builder(200).body(body).build())
}

/// route: the main websocket connection consumed by the ui.
async fn ws(
  request: tide::Request<shared_state::SharedState>,
  mut connection: tide_websockets::WebSocketConnection,
) -> tide::Result<()> {
  let state = request.state();
  let authority = match utils::cookie_claims(&request) {
    None => return Err(tide::Error::from_str(404, "not-found")),
    Some(claims) => state.authority(claims.oid).await,
  };

  if authority != Some(sec::Authority::Admin) {
    tracing::warn!("non-admin attempt to open websocket, refusing");
    return Err(tide::Error::from_str(404, "not-found"));
  }

  let span = tracing::span!(parent: &state.span, tracing::Level::INFO, "websocket");
  let _ = span.enter();

  let (sender, receiver) = channel::unbounded();

  tracing::info!("websocket client connected");
  let id = uuid::Uuid::new_v4().to_string();
  state.messages.send(Message::ClientConnected(id.clone())).await?;
  state.registration.send((id.clone(), sender)).await?;

  /// During our interval, we'll either be receiving string data from the connection, or a command
  /// to send into the connection. We'll race these two effects and perform the correct action
  /// based on which finishes first.
  enum FrameResult {
    /// Wraps the effect runtime command.
    Command(Command),

    /// Wraps the effect runtime message. Is ultimately mapped into a `Message::ClientData` kind.
    Message(String),
  }

  loop {
    let application_input = async {
      // Attempt to receive any client-bound command sent from the application runtime.
      match receiver.recv().await {
        Err(error) => {
          tracing::warn!("unable to receive inside websocket - {error}");
          Err(io::Error::new(
            io::ErrorKind::Other,
            format!("unable to receive command - {error}"),
          ))
        }
        Ok(command) => Ok(Some(FrameResult::Command(command))),
      }
    };

    let client_input = async {
      match connection.next().await {
        None => Err(io::Error::new(io::ErrorKind::Other, "end-of-stream")),
        Some(Ok(tide_websockets::Message::Text(data))) => {
          tracing::info!("has data from websocket - {data}");
          Ok(Some(FrameResult::Message(data)))
        }
        Some(Ok(_)) => Ok(None),
        Some(Err(error)) => {
          tracing::warn!("failed reading from client websocket - {error}");
          Err(io::Error::new(
            io::ErrorKind::Other,
            format!("unable to receive from client - {error}"),
          ))
        }
      }
    };

    match client_input.race(application_input).await {
      Ok(Some(FrameResult::Message(data))) => {
        if let Err(error) = request
          .state()
          .messages
          .send(Message::ClientData(id.clone(), data))
          .await
        {
          tracing::warn!("unable to send client data though message channel - {error}");
          break;
        }
      }
      Ok(Some(FrameResult::Command(Command::SendState(_, data)))) => {
        if let Err(error) = connection.send_string(data).await {
          tracing::warn!("unable to send serialized command to client - {error}");
          break;
        }
      }
      Ok(None) => tracing::debug!("todo"),
      Err(error) => {
        tracing::warn!("invalid client websocket interval - {error}");
        break;
      }
    }
  }

  state.messages.send(Message::ClientDisconnected(id.clone())).await?;
  Ok(())
}

/// Internal to the module package, the `ServerRuntime` is responsible for creating the tide
/// application, registering the routes and actually binding the tcp listener.
struct ServerRuntime {
  /// The top-level http effect runtime configuration.
  config: configuration::Configuration,

  /// A pair of channels that are proxied in the `Http` effect manager and forwarded along from/to
  /// the concrete application runtime.
  channels: (channel::Sender<Message>, channel::Receiver<Command>),
}

impl ServerRuntime {
  /// Creates a new runtime from a configuration and a pair of proxied message/command channels.
  fn new(
    config: configuration::Configuration,
    channels: (channel::Sender<Message>, channel::Receiver<Command>),
  ) -> Self {
    Self { config, channels }
  }

  /// Responsible for registering all of our `tide` application routes
  async fn run(self) -> io::Result<()> {
    let span = tracing::span!(tracing::Level::INFO, "http/web");
    let _ = span.enter();

    let (reg_sender, reg_receiver) = channel::unbounded();

    let mut app = tide::with_state(shared_state::SharedState {
      config: self.config.clone(),
      redis: async_std::sync::Arc::new(async_std::sync::Mutex::new(None)),
      messages: self.channels.0.clone(),
      registration: reg_sender,
      span,
    });
    app.at("/status").get(heartbeat);
    app.at("/ws").with(tide_websockets::WebSocket::new(ws)).get(heartbeat);

    app.at("/auth/start").get(auth_routes::start);
    app.at("/auth/end").get(auth_routes::end);
    app.at("/auth/complete").get(auth_routes::complete);
    app.at("/auth/identify").get(auth_routes::identify);
    app.at("/upload").post(file_routes::upload);

    // Our proxy task/future here is responsible for managing the mapping of client ids with a
    // channel that can be used to send them `Command`s.
    let proxy_task = async {
      let (_, commands) = self.channels;
      let clients: std::collections::HashMap<String, channel::Sender<Command>> = std::collections::HashMap::new();
      let locked = sync::Arc::new(sync::Mutex::new(clients));

      loop {
        // We'll be racing two futures every loop. First, we'll attempt to pull a command off our
        // inbound command channel. If there is one, we will propagate it through to the other
        // channels.
        let cmd = async {
          let clients = locked.clone();

          // Pull off any available command
          let command = match commands.recv().await {
            Err(error) => {
              tracing::warn!("unable to receive in web command proxy task - {error}");
              return Err(error);
            }
            Ok(c) => c,
          };

          // Match on the command to get access to the underlying id that we want to send to, and
          // then send the command to that client.
          match &command {
            Command::SendState(id, _) => {
              tracing::info!("received state publish command - {id}");
              let clients = clients.lock().await;

              if let Some(sender) = clients.get(id) {
                if let Err(error) = sender.send(command.clone()).await {
                  tracing::warn!("failed comand propagation - {error}");
                }
              }
            }
          }

          Ok(())
        };

        // The second future here is an attempt to pull any new clients off our "registration"
        // channel. These new clients will be added to the the hash that is managed in this
        // thread/task.
        let rec = async {
          let clients = locked.clone();

          match reg_receiver.recv().await {
            Ok((id, sender)) => {
              tracing::info!("has new client - {id}");
              let mut clients = clients.lock().await;
              clients.insert(id, sender);
              Ok(())
            }
            Err(error) => {
              tracing::warn!("unable to receive registration - {error}");
              Err(error)
            }
          }
        };

        if let Err(error) = cmd.race(rec).await {
          tracing::warn!("breaking server command loop - {error}");
          break;
        }
      }

      Ok(())
    };

    app.listen(&self.config.addr).race(proxy_task).await
  }
}
