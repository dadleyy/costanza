//! Auth0 type definitions and some wrapping structs that are json serializable.

use serde::{Deserialize, Serialize};
use std::io::{Error, ErrorKind, Result};

#[allow(clippy::missing_docs_in_private_items)]
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UserRole {
  id: String,
  name: String,
}

impl UserRole {
  /// Will return if the given rule is should be consider an "admin" role.
  pub fn is_admin(&self) -> bool {
    self.name.split(':').any(|part| part.starts_with("admin"))
  }
}

#[allow(clippy::missing_docs_in_private_items)]
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UserInfo {
  pub sub: String,
  pub nickname: String,
  pub email: String,
  pub picture: String,
  pub email_verified: Option<bool>,
}

#[allow(clippy::missing_docs_in_private_items)]
#[derive(Debug, Deserialize)]
struct AuthCodeResponse {
  access_token: String,
}

#[allow(clippy::missing_docs_in_private_items)]
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ManagementTokenResponse {
  access_token: String,
}

#[allow(clippy::missing_docs_in_private_items)]
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ManagementUserInfoResponse {
  name: Option<String>,
  pub(crate) user_id: String,
  picture: Option<String>,
  email: Option<String>,
  nickname: Option<String>,
  email_verified: bool,
}

#[allow(clippy::missing_docs_in_private_items)]
#[derive(Debug, Serialize)]
pub struct AuthCodeRequest {
  grant_type: String,
  client_id: String,
  client_secret: String,
  redirect_uri: Option<String>,
  code: Option<String>,
  audience: Option<String>,
}

#[allow(clippy::missing_docs_in_private_items)]
#[derive(Debug, Clone, Deserialize)]
pub struct AuthZeroConfig {
  auth_client_id: String,
  auth_client_secret: String,
  management_client_id: String,
  management_client_secret: String,
  redirect_uri: String,
  domain: String,
}

impl AuthZeroConfig {
  /// Returns the url that is used for exchanging codes for tokens.
  #[inline]
  pub fn token_uri(&self) -> Result<String> {
    let base = format!("{}/oauth/token", self.domain);
    Ok(base)
  }

  /// Loads the basic oauth information provided from auth0.
  pub async fn fetch_initial_user_info<T>(&self, code: T) -> Result<UserInfo>
  where
    T: std::fmt::Display,
  {
    let mut response = surf::post(&self.token_uri()?)
      .body_json(&self.auth_token_payload(code)?)
      .map_err(|error| {
        tracing::warn!("unable to serialize auth token payload - {}", error);
        Error::new(ErrorKind::Other, "bad-token-serialize")
      })?
      .await
      .map_err(|error| {
        tracing::warn!("unable to request token for code - {}", error);
        Error::new(ErrorKind::Other, "bad-code-exchange-request")
      })?;

    let tok = response
      .body_json::<AuthCodeResponse>()
      .await
      .map_err(|error| {
        tracing::warn!("unable to parse token exchange response - {}", error);
        Error::new(ErrorKind::Other, "bad-code-exchange-request")
      })
      .map(|body| body.access_token)?;

    let mut res = surf::get(format!("{}/userinfo", self.domain))
      .header("Authorization", format!("Bearer {}", tok))
      .await
      .map_err(|error| {
        tracing::warn!("unable to parse token exchange response - {}", error);
        Error::new(ErrorKind::Other, "bad-code-exchange-request")
      })?;

    res.body_json::<UserInfo>().await.map_err(|error| {
      tracing::warn!("unable to parse token exchange response - {}", error);
      Error::new(ErrorKind::Other, "bad-code-exchange-request")
    })
  }

  /// Loads detailed user information from the auth0 management api.
  pub async fn fetch_detailed_user_info<T>(&self, id: T) -> Result<ManagementUserInfoResponse>
  where
    T: std::fmt::Display,
  {
    let token = self.get_new_management_token().await?;
    let mut response = surf::get(format!("{}/api/v2/users/{}", self.domain, id))
      .header("Authorization", format!("Bearer {}", token))
      .await
      .map_err(|error| {
        tracing::warn!("unable to parse user info response - {}", error);
        Error::new(ErrorKind::Other, format!("{}", error))
      })?;

    if response.status() != surf::StatusCode::Ok {
      return Err(Error::new(ErrorKind::Other, "not-ok-response"));
    }

    response
      .body_json::<ManagementUserInfoResponse>()
      .await
      .map_err(|error| {
        tracing::warn!("unable to parse response - {}", error);
        Error::new(ErrorKind::Other, format!("{}", error))
      })
  }

  /// Loads user roles from the auth0 management api for a given user.
  pub async fn fetch_user_roles<T>(&self, id: T) -> Result<Vec<UserRole>>
  where
    T: std::fmt::Display,
  {
    let token = self.get_new_management_token().await?;
    let mut response = surf::get(format!("{}/api/v2/users/{}/roles", self.domain, id))
      .header("Authorization", format!("Bearer {}", token))
      .await
      .map_err(|error| {
        tracing::warn!("unable to parse user info response - {}", error);
        Error::new(ErrorKind::Other, format!("{}", error))
      })?;

    tracing::debug!("request for roles completed - {}", response.status());

    response.body_json::<Vec<UserRole>>().await.map_err(|error| {
      tracing::warn!("unable to parse user role response - {}", error);
      Error::new(ErrorKind::Other, format!("{}", error))
    })
  }

  /// Will attempt to create a management token for querying the auth0 management api.
  async fn get_new_management_token(&self) -> Result<String> {
    let mut response = surf::post(&self.token_uri()?)
      .body_json(&self.manage_token_payload()?)
      .map_err(|error| {
        tracing::warn!("failed serializing management token payload - {}", error);
        Error::new(ErrorKind::Other, "bad-management-payload")
      })?
      .await
      .map_err(|error| {
        tracing::warn!("failed management token response - {}", error);
        Error::new(ErrorKind::Other, "bad-management-response")
      })?;

    if response.status() != surf::StatusCode::Ok {
      return Err(Error::new(ErrorKind::Other, "not-ok-response"));
    }

    response
      .body_json::<ManagementTokenResponse>()
      .await
      .map_err(|error| {
        tracing::warn!("unable to parse response - {}", error);
        Error::new(ErrorKind::Other, format!("{}", error))
      })
      .map(|b| b.access_token)
  }

  /// Returns the json payload that will be used to look up Aut0 management info for a given user
  /// id.
  fn manage_token_payload(&self) -> Result<AuthCodeRequest> {
    Ok(AuthCodeRequest {
      client_id: self.management_client_id.clone(),
      client_secret: self.management_client_secret.clone(),
      code: None,
      redirect_uri: None,
      grant_type: "client_credentials".into(),
      audience: Some(format!("{}/api/v2/", self.domain)),
    })
  }

  /// Creates the oauth payload for exchanging a code into a token.
  fn auth_token_payload<T>(&self, code: T) -> Result<AuthCodeRequest>
  where
    T: std::fmt::Display,
  {
    Ok(AuthCodeRequest {
      client_id: self.auth_client_id.clone(),
      client_secret: self.auth_client_secret.clone(),
      code: Some(format!("{}", code)),
      audience: None,
      redirect_uri: Some(self.redirect_uri.clone()),
      grant_type: "authorization_code".into(),
    })
  }

  /// Returns the url that users will be sent to at the start of an oauth exchange.
  pub fn redirect_uri(&self) -> Result<String> {
    let base = format!("{}/authorize", self.domain);
    tide::http::Url::parse_with_params(
      &base,
      &[
        ("client_id", self.auth_client_id.as_str()),
        ("redirect_uri", self.redirect_uri.as_str()),
        ("response_type", "code"),
        ("scope", "openid profile email"),
      ],
    )
    .map_err(|error| {
      tracing::warn!("unable to build redirect uri - {}", error);
      Error::new(ErrorKind::Other, "bad-oauth-redirect-uri")
    })
    .map(|url| url.to_string())
  }
}
