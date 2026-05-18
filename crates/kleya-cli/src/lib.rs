//! kleya-cli: command-line entry point (binary + supporting library).

#![allow(
    missing_docs,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions
)]

pub mod clap_args;
pub mod config_loader;
pub mod dispatch;
pub mod exit_code;
pub mod key_store_fs;
pub mod logging;
pub mod ssh_probe;
