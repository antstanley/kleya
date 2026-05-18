#![allow(missing_docs, clippy::unwrap_used)]

use clap::Parser as _;
use kleya_cli::clap_args::Cli;
use kleya_core::test_support::{InMemoryCompute, InMemoryKeyStore};
use std::sync::Arc;

#[tokio::test]
async fn list_subcommand_runs_against_fake_with_no_instances() {
    let cli = Cli::parse_from(["kleya", "list"]);
    let cfg = Arc::new(kleya_core::Config::default());
    let compute: Arc<dyn kleya_core::ports::cloud_compute::CloudCompute> =
        Arc::new(InMemoryCompute::new());
    let key_store: Arc<dyn kleya_core::ports::key_store::KeyStore> =
        Arc::new(InMemoryKeyStore::new());
    kleya_cli::dispatch::run_with(cli, cfg, "eu-west-1".into(), compute, key_store)
        .await
        .expect("ok");
}
