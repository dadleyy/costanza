use super::{shared_state, utils};

/// route: attempts to parse the request body as a raw utf-8 string and pass the contents over the
/// outbound message channel to be picked up by the concrete application runtime.
pub(super) async fn upload(mut request: tide::Request<shared_state::SharedState>) -> tide::Result {
  let claims = utils::cookie_claims(&request);

  if claims.is_none() {
    tracing::warn!("missing claims on request to upload file");
    return Ok(tide::Response::new(404));
  }

  let claims = claims.unwrap();
  let session_data = request.state().user_from_session(&claims.oid).await.ok_or_else(|| {
    tracing::warn!("unable to load session data for claims {}", claims.oid);
    tide::Error::from_str(404, "no-session")
  })?;

  let content_type = request
    .content_type()
    .ok_or_else(|| tide::Error::from_str(422, "missing-filetype"))?;

  if content_type.basetype() != "text" {
    tracing::warn!("invalid upload type - {content_type:?}");
    return Err(tide::Error::from_str(422, "invalid-filetype"));
  }

  let size = request.len().unwrap_or(0);
  if size == 0 || size > 3600 {
    tracing::warn!("invalid request size - {size}");
    return Err(tide::Error::from_str(422, "file-too-large"));
  }

  tracing::info!("file upload initiated by '{}'", session_data.user.user_id,);
  let bytes = request.body_bytes().await?;
  let raw = String::from_utf8(bytes).map_err(|error| {
    tracing::warn!("unable to interpret upload as valid utf8-string: {error}");
    tide::Error::from_str(422, "invalid-file")
  })?;
  tracing::info!("raw byte contents as string - '{raw:?}'");

  request
    .state()
    .messages
    .send(super::Message::FileUpload(raw))
    .await
    .map_err(|error| {
      tracing::warn!("unable to interpret upload as valid utf8-string: {error}");
      tide::Error::from_str(422, "invalid-file")
    })?;

  Ok(tide::Response::new(200))
}
