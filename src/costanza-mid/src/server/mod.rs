use async_std::{channel, sync};
use futures_lite::{FutureExt, StreamExt};
use serde::Serialize;
use std::io;

mod auth_routes;
mod configuration;
mod constants;
mod oauth;
mod sec;
mod shared_state;
mod utils;

pub use configuration::Configuration;

#[derive(Debug, PartialEq, Eq, Clone, Serialize)]
pub enum Command {
  SendState(String, String),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Message {
  ClientConnected(String),
  ClientDisconnected(String),
}

#[derive(Serialize)]
struct Heartbeat {
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
    Command(Command),
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
        Ok(command) => {
          tracing::info!("has command to send to websocket - {command:?}");
          Ok(Some(FrameResult::Command(command)))
        }
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
      Ok(Some(FrameResult::Command(command))) => {
        if let Err(error) = connection.send_json(&command).await {
          tracing::warn!("unable to send serialized command to client - {error}");
          break;
        }
      }
      Ok(Some(_)) => tracing::info!("todo"),
      Ok(None) => tracing::info!("todo"),
      Err(error) => {
        tracing::warn!("invalid client websocket interval - {error}");
        break;
      }
    }
  }

  state.messages.send(Message::ClientDisconnected(id.clone())).await?;
  Ok(())
}

pub struct ServerRuntime {
  config: configuration::Configuration,
  channels: (channel::Sender<Message>, channel::Receiver<Command>),
}

impl ServerRuntime {
  pub fn new(
    config: configuration::Configuration,
    channels: (channel::Sender<Message>, channel::Receiver<Command>),
  ) -> Self {
    Self { config, channels }
  }

  pub async fn run(self) -> io::Result<()> {
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

    app
      .listen(&self.config.addr)
      .race(async {
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
              _ => (),
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
      })
      .await
  }
}
