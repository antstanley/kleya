//! kleya-core: domain types, ports, and command orchestration.
//!
//! This crate is free of I/O and provider SDKs. Adapters live in sibling crates.

#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::non_std_lazy_statics
)]

pub mod error;
pub mod limits;
pub mod model;

pub use error::{Error, Result};
