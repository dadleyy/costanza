/// The name of our session cookie used within our `Set-Cookie` headers.
pub(super) const COOKIE_NAME: &str = "_costanza_session";

/// When setting the cookie, these flags are used alongside the actual value.
#[cfg(debug_assertions)]
pub(super) const COOKIE_SET_FLAGS: &str = "Max-Age=3600; Path=/; SameSite=Strict; HttpOnly";
#[cfg(not(debug_assertions))]
pub(super) const COOKIE_SET_FLAGS: &str = "Max-Age=3600; Path=/; SameSite=Strict; HttpOnly; Secure";

/// When clearing a cookie, these flags are sent.
#[cfg(debug_assertions)]
pub(super) const COOKIE_CLEAR_FLAGS: &str =
  "Max-Age=0; Expires=Wed, 21 Oct 2015 07:28:00 GMT; Path=/; SameSite=Strict; HttpOnly";
#[cfg(not(debug_assertions))]
pub(super) const COOKIE_CLEAR_FLAGS: &str =
  "Max-Age=0; Expires=Wed, 21 Oct 2015 07:28:00 GMT; Path=/; SameSite=Strict; HttpOnly; Secure";
