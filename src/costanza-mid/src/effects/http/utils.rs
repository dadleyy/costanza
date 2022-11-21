//! Not quite sure where this should live yet. It is functionality that will probably be shared
//! across multiple routes but does not "belong" on the shared state since it may operate on data
//! in the request that contains that.

use super::{constants, sec, shared_state};

/// Returns the cookie responsible for holding our session from the request http header.
pub(super) fn cookie_claims(request: &tide::Request<shared_state::SharedState>) -> Option<sec::Claims> {
  request
    .cookie(constants::COOKIE_NAME)
    .and_then(|cook| sec::Claims::decode(&cook.value(), &request.state().config.session.jwt_secret).ok())
}
