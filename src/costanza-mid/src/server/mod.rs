use futures_lite::StreamExt;
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
  connection: tide_websockets::WebSocketConnection,
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
  tracing::event!(parent: &span, tracing::Level::INFO, "handling websocket, yay");
  let mut interval = async_std::stream::interval(std::time::Duration::from_millis(1000));

  loop {
    interval.next().await;

    if let Err(error) = connection
      .send_json(&Heartbeat {
        time: std::time::SystemTime::now(),
      })
      .await
    {
      tracing::event!(
        parent: &span,
        tracing::Level::INFO,
        "unable to send json heartbeat - {error}"
      );
      break;
    }

    tracing::event!(parent: &span, tracing::Level::WARN, "websocket heartbeat sent");
  }

  Ok(())
}

pub async fn start(config: configuration::Configuration) -> io::Result<()> {
  let span = tracing::span!(tracing::Level::INFO, "http/web");
  let _ = span.enter();

  let mut app = tide::with_state(shared_state::SharedState {
    config: config.clone(),
    redis: async_std::sync::Arc::new(async_std::sync::Mutex::new(None)),
    span,
  });
  app.at("/status").get(heartbeat);
  app.at("/ws").with(tide_websockets::WebSocket::new(ws)).get(heartbeat);

  app.at("/auth/start").get(auth_routes::start);
  app.at("/auth/end").get(auth_routes::end);
  app.at("/auth/complete").get(auth_routes::complete);
  app.at("/auth/identify").get(auth_routes::identify);

  app.listen(&config.addr).await
}
