use super::{constants, sec, shared_state, utils};
use serde::Serialize;

/// The json-serializable response structure for our identify endpoint.
#[derive(Debug, Serialize)]
struct AuthIdentifyResponse {
  /// This field is true when were are able to verify an authenticated user from the cookie data.
  ok: bool,

  /// Include the version in our auth payload.
  version: String,

  /// The current time.
  timestamp: chrono::DateTime<chrono::Utc>,

  /// Optionally-included information about the user if we found one.
  session: Option<sec::AuthIdentifyResponseUserInfo>,
}

impl Default for AuthIdentifyResponse {
  fn default() -> Self {
    Self {
      ok: false,
      timestamp: chrono::Utc::now(),
      session: None,
      version: "unknown".to_string(),
    }
  }
}

/// route: oauth flow redirect.
pub(super) async fn start(request: tide::Request<shared_state::SharedState>) -> tide::Result {
  tracing::info!("initializing oauth redirect");
  let destination = request.state().config.oauth.redirect_uri().map_err(|error| {
    tracing::warn!("{}", error);
    tide::Error::from_str(500, "bad-oauth")
  })?;

  Ok(tide::Redirect::temporary(destination).into())
}

/// route: oauth token -> user information exchange. also creates a redis session entry and returns
/// a cookie to the browser.
pub(super) async fn complete(request: tide::Request<shared_state::SharedState>) -> tide::Result {
  let code = request
    .url()
    .query_pairs()
    .find_map(|(k, v)| if k == "code" { Some(v) } else { None })
    .ok_or_else(|| tide::Error::from_str(404, "no-code"))?;

  let oauth = &request.state().config.oauth;

  // Swap our code for a token and load the basic user information it provides for us.
  let user = oauth.fetch_initial_user_info(&code).await.map_err(|error| {
    tracing::warn!("unable to fetch initial user info - {}", error);
    tide::Error::from_str(500, "bad-oauth")
  })?;

  if user.email_verified.is_none() {
    tracing::warn!("user email not verified for sub '{}'", user.sub);
    return Err(tide::Error::from_str(404, "user-not-found"));
  }

  // Fetch the Auth0 roles for this user.
  let roles = oauth.fetch_user_roles(&user.sub).await.map_err(|error| {
    tracing::warn!("unable to fetch user roles - {}", error);
    tide::Error::from_str(500, "bad-roles-listing")
  })?;

  // TODO: should non-admins be allowed to see info?
  if !roles.iter().any(|role| role.is_admin()) {
    tracing::warn!("user not admin, skippping cookie setting (roles {:?})", roles);
    return Err(tide::Error::from_str(404, "user-not-found"));
  }

  // Fetch the complete user information available to us from the auth0 api.
  let user = oauth.fetch_detailed_user_info(&user.sub).await.map_err(|error| {
    tracing::warn!("unable to load complete user information from auth0 - {error}");
    error
  })?;

  // Create a serializable representation of our user information
  let session_data = sec::AuthIdentifyResponseUserInfo { user, roles };
  let session_id = uuid::Uuid::new_v4().to_string();
  let serialized_session = serde_json::to_string(&session_data).map_err(|error| {
    tracing::warn!("unable to serialize session data - {error}");
    error
  })?;

  // Perist that user information into our redis storage.
  let command = kramer::Command::Strings(kramer::StringCommand::Set(
    kramer::Arity::One((&session_id, &serialized_session)),
    None,
    kramer::Insertion::Always,
  ));

  request.state().command(command).await.map_err(|error| {
    tracing::warn!("unable to persist session information - {error}");
    error
  })?;

  // Create our json web token, including the unique identifier we generated for this session.
  let jwt = sec::Claims::for_sub(&session_id).encode(&request.state().config.session.jwt_secret)?;
  let cookie = format!(
    "{}={}; {}; Domain={}",
    constants::COOKIE_NAME,
    jwt,
    constants::COOKIE_SET_FLAGS,
    &request.state().config.domain
  );

  // TODO - determine where to send the user. Once the web UI is created, we will send the user to some login page
  // where an attempt will be made to fetch identity information using the newly-set cookie.
  let response = tide::Response::builder(302)
    .header("Set-Cookie", cookie)
    .header("Location", request.state().config.auth_complete_uri.as_str())
    .build();

  Ok(response)
}

/// route: return user darta based on the information available to us in the cookie.
pub(super) async fn identify(request: tide::Request<shared_state::SharedState>) -> tide::Result {
  let claims = utils::cookie_claims(&request);

  tracing::info!("attempting to identify user from claims - {:?}", claims);

  let mut res = AuthIdentifyResponse {
    version: "".to_string(),
    ..Default::default()
  };

  if let Some(claims) = claims {
    let session_data = request.state().user_from_session(&claims.oid).await.ok_or_else(|| {
      tracing::warn!("unable to load session data for claims {}", claims.oid);
      tide::Error::from_str(404, "no-session")
    })?;

    if session_data.roles.iter().any(|role| role.is_admin()) {
      res.ok = true;
      res.session = Some(session_data);
    }
  }

  tide::Body::from_json(&res).map(|bod| tide::Response::builder(200).body(bod).build())
}

/// route: clear the cookie and redirect users back to the ui.
pub(super) async fn end(request: tide::Request<shared_state::SharedState>) -> tide::Result {
  let claims = utils::cookie_claims(&request);

  if let Some(inner) = claims {
    tracing::debug!("attempting to delete session for '{}'", inner.oid);

    if let Err(error) = request
      .state()
      .command(kramer::Command::Del::<&str, &str>(kramer::Arity::One(&inner.oid)))
      .await
    {
      tracing::error!("unable to dleete session data - '{error}'");
    }
  }

  let clear_cookie = format!(
    "{}=''; {}; Domain={}",
    constants::COOKIE_NAME,
    constants::COOKIE_CLEAR_FLAGS,
    request.state().config.domain
  );

  let response = tide::Response::builder(302)
    .header("Set-Cookie", &clear_cookie)
    .header("Location", &request.state().config.auth_complete_uri)
    .build();

  tracing::debug!("clearing session cookie via {clear_cookie}");

  Ok(response)
}
