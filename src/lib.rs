//! Core Animation Data Layer -- shared between the desktop and browser pipelines.
//! Pure `std`, no platform-specific code: this crate needs to compile natively for the desktop
//! pipeline and to wasm32 for the browser pipeline without any `#[cfg]` forks.

pub mod ledger;
pub mod import;
pub use crate::import::edge;

pub use ledger::{EntityId, Ledger};
