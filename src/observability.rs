use miette::IntoDiagnostic;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Setup the global logger with the given level filter
pub(crate) fn setup_logger(level_filter: LevelFilter) -> miette::Result<()> {
    // filter for low-level/depedency logs
    let low_level_filter = match level_filter {
        LevelFilter::OFF => LevelFilter::OFF,
        LevelFilter::ERROR => LevelFilter::ERROR,
        LevelFilter::WARN => LevelFilter::WARN,
        LevelFilter::INFO => LevelFilter::WARN,
        LevelFilter::DEBUG => LevelFilter::INFO,
        LevelFilter::TRACE => LevelFilter::TRACE,
    };

    let mut layer_env_filter = EnvFilter::builder()
        .with_default_directive(level_filter.into())
        .from_env()
        .into_diagnostic()?;

    layer_env_filter = layer_env_filter
        // add low-level filter for hyper_util/reqwest
        .add_directive(
            format!("hyper_util={}", low_level_filter)
                .parse()
                .into_diagnostic()?,
        )
        .add_directive(
            format!("reqwest={}", low_level_filter)
                .parse()
                .into_diagnostic()?,
        );

    let layer_fmt = tracing_subscriber::fmt::layer().without_time();

    tracing_subscriber::registry()
        .with(layer_env_filter)
        .with(layer_fmt)
        .init();

    Ok(())
}
