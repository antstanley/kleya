//! kleya binary entry point.

use clap::Parser as _;
use std::process::ExitCode;
use tokio_util::sync::CancellationToken;

use kleya_cli::{clap_args, dispatch, exit_code, logging};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = clap_args::Cli::parse();
    logging::init(
        cli.verbose,
        matches!(cli.log_format, clap_args::LogFormat::Json),
    );
    let cancel = CancellationToken::new();
    let cancel_for_signal = cancel.clone();
    tokio::spawn(async move {
        if let Ok(()) = tokio::signal::ctrl_c().await {
            tracing::warn!("SIGINT received; cancelling outstanding work");
            cancel_for_signal.cancel();
        }
    });
    match dispatch::run(cli, cancel).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e);
            ExitCode::from(u8::try_from(exit_code::code_for(&e)).unwrap_or(1))
        }
    }
}
