use serde::Deserialize;

/// The session store config
#[derive(Deserialize, Debug, Clone)]
pub(super) struct SessionStoreConfiguration {
  /// A secret that will be used to sign JWT tokens.
  pub(super) jwt_secret: String,

  /// The address that we can find redis at. Used for storing user data.
  pub(super) redis_addr: String,
}

/// The main configuration schema for the http effect runtime.
#[derive(Deserialize, Debug, Clone)]
pub struct Configuration {
  /// The address to bind our tcp stream to.
  pub(super) addr: String,

  /// The domain that cookies will be bound to
  pub(super) domain: String,

  /// Where users will be sent on successful oauth.
  pub(super) auth_complete_uri: String,

  /// Configuration used for authentication.
  pub(super) session: SessionStoreConfiguration,

  /// Configuration used for authorization.
  pub(super) oauth: super::oauth::AuthZeroConfig,
}
