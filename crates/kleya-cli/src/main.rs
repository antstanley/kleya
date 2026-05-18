//! kleya binary entry point.

use clap::Parser as _;
use std::process::ExitCode;

use kleya_cli::{clap_args, dispatch, exit_code, logging};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = clap_args::Cli::parse();
    logging::init(
        cli.verbose,
        matches!(cli.log_format, clap_args::LogFormat::Json),
    );
    match dispatch::run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e);
            ExitCode::from(u8::try_from(exit_code::code_for(&e)).unwrap_or(1))
        }
    }
}
