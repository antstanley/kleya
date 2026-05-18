//! kleya-aws: EC2 adapter for `kleya_core::ports::CloudCompute`.

#![allow(
    missing_docs,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions
)]

pub mod client;
pub mod ec2;
pub mod error;
pub mod mapping;
