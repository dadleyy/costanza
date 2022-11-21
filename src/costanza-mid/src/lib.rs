#![forbid(unsafe_code)]

//! This library contains the various "effect runtimes" that are used by the application itself.

/// This module is responsible for providing a generic structure that any "side-effect runtime"
/// like the http and serial managers would implement.
pub mod eff;

/// The `effects` module actually contains the concrete implementations of the generic types
/// exposed in the `eff` module.
pub mod effects;
