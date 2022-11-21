//! This module is a the wrapper around our oauth types from auth0 that deal specifically with how
//! we store that information in our session + json web token.

use super::oauth;
use serde::{Deserialize, Serialize};

/// Based on the cookie provided to our http endpoints, the `Authority` here represents what access
/// the user should be allowed to have.
#[derive(PartialEq)]
pub(super) enum Authority {
  /// Currently, only an admin/non-admin distinction is made based on Auth0 management api role
  /// associations.
  Admin,
}

/// The inner type sent in our identify endpoint when a user is available.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct AuthIdentifyResponseUserInfo {
  /// Contains information provided by our oauth provider.
  pub(super) user: oauth::ManagementUserInfoResponse,

  /// The list of Auth0 roles our current user is assigned to.
  pub(crate) roles: Vec<oauth::UserRole>,
}

/// Information that is included in our JWT claims.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Claims {
  /// The `exp` field is used for expiring tokens at some point in time.
  pub(crate) exp: usize,
  /// The `iat` field holds when the jwt was created.
  pub(crate) iat: usize,
  /// The id of our authenticated user.
  pub(crate) oid: String,
}

impl Claims {
  /// Given an id of a user, will return an instance of our claims with all other fields
  /// populated.
  pub fn for_sub<T>(oid: T) -> Self
  where
    T: std::fmt::Display,
  {
    let day = chrono::Utc::now()
      .checked_add_signed(chrono::Duration::minutes(60))
      .unwrap_or_else(chrono::Utc::now);

    let exp = day.timestamp() as usize;
    let iat = chrono::Utc::now().timestamp() as usize;
    tracing::debug!("encoding new jwt, expires {}", exp);

    Self {
      exp,
      iat,
      oid: format!("{}", oid),
    }
  }

  /// Given the value of a jwt represented in some string-able type, will return the decoded
  /// representation.
  pub fn decode<T, S>(target: &T, secret: &S) -> std::io::Result<Self>
  where
    T: std::fmt::Display,
    S: std::convert::AsRef<str>,
  {
    let token = format!("{}", target);
    let key = jsonwebtoken::DecodingKey::from_secret(secret.as_ref().as_bytes());
    let validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
    jsonwebtoken::decode::<Self>(token.as_str(), &key, &validation)
      .map_err(|error| {
        tracing::warn!("unable to decode token - {}", error);
        std::io::Error::new(std::io::ErrorKind::Other, "bad-jwt")
      })
      .map(|data| data.claims)
  }

  /// Encodes our claims into their string form.
  pub fn encode<S>(&self, secret: S) -> std::io::Result<String>
  where
    S: std::convert::AsRef<str>,
  {
    let header = &jsonwebtoken::Header::default();
    let secret = jsonwebtoken::EncodingKey::from_secret(secret.as_ref().as_bytes());

    jsonwebtoken::encode(header, &self, &secret).map_err(|error| {
      tracing::warn!("unable to encode token - {}", error);
      std::io::Error::new(std::io::ErrorKind::Other, "bad-jwt")
    })
  }
}
