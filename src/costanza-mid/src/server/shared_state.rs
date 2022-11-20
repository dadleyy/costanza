//! This module contains the main type that is shared across request tasks.

use super::sec;
use async_std::{channel, sync};
use serde::Serialize;
use std::io;

#[derive(Clone)]
pub struct SharedState {
  pub(super) config: super::configuration::Configuration,

  pub(super) redis: sync::Arc<sync::Mutex<Option<async_std::net::TcpStream>>>,

  pub(super) messages: channel::Sender<super::Message>,

  pub(super) registration: channel::Sender<(String, channel::Sender<super::Command>)>,

  pub(super) span: tracing::Span,
}

impl SharedState {
  /// Executes a redis command against our shared, mutex locked redis "pool".
  pub(super) async fn command<K, V>(&self, command: kramer::Command<K, V>) -> io::Result<kramer::Response>
  where
    K: std::fmt::Display,
    V: std::fmt::Display,
  {
    let mut redis = self.redis.lock().await;

    let mut pulled_connection = match redis.take() {
      Some(inner) => inner,
      None => async_std::net::TcpStream::connect(&self.config.session.redis_addr)
        .await
        .map_err(|error| {
          tracing::error!("failed establishing new connection to redis - {error}");
          error
        })?,
    };

    let output = kramer::execute(&mut pulled_connection, &command)
      .await
      .map_err(|error| {
        tracing::error!("unable to execute redis command - {error}");
        error
      })?;

    *redis = Some(pulled_connection);

    Ok(output)
  }

  /// Returns the authority level based on the session data provided by our cookie. This is
  /// verified against our external oauth (auth0) provider.
  pub(super) async fn authority<T>(&self, id: T) -> Option<sec::Authority>
  where
    T: std::fmt::Display,
  {
    let data = self.user_from_session(id).await?;

    if data.roles.into_iter().any(|role| role.is_admin()) {
      return Some(sec::Authority::Admin);
    }

    None
  }

  /// This function is responsible for taking the unique id found in our session cookie and
  /// returning the user data that we have previously stored in redis.
  pub(crate) async fn user_from_session<T>(&self, id: T) -> Option<sec::AuthIdentifyResponseUserInfo>
  where
    T: std::fmt::Display,
  {
    // Look up our session by the uuid in our redis session store
    let serialized_id = format!("{id}");
    let command =
      kramer::Command::Strings::<&str, &str>(kramer::StringCommand::Get(kramer::Arity::One(&serialized_id)));

    let response = self
      .command(command)
      .await
      .map_err(|error| {
        tracing::error!("unable to fetch session info - {error}");
        error
      })
      .ok()?;

    // Attempt to deserialize as our user info structure.
    if let kramer::Response::Item(kramer::ResponseValue::String(inner)) = response {
      tracing::trace!("has session data - {inner:?}");
      return serde_json::from_str(&inner).ok();
    }

    None
  }
}
