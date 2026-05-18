use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init(verbosity: u8, json: bool) {
    let level = match verbosity {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("kleya={level},warn")));
    if json {
        tracing_subscriber::registry()
            .with(fmt::layer().json().with_target(false))
            .with(filter)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(fmt::layer().with_target(false))
            .with(filter)
            .init();
    }
}
