use crate::Logging;
use std::sync::OnceLock;
use std::{fs, path::Path};
use tracing_error::ErrorLayer;
use tracing_subscriber::{fmt, prelude::*};

static FILE_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

/// Initialize tracing for binaries.
///
/// - Honors `RUST_LOG` if set.
/// - Falls back to `config.level` (typically `"info"`).
pub fn init_tracing(config: &Logging) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| config.level.clone().into());
    let mut file_guard_slot = None;
    let mut writer = None;

    if let Some(dir) = config.data_dir.as_ref() {
        if let Err(e) = fs::create_dir_all(Path::new(dir)) {
            return eprintln!("Failed to create SOMA_LOGS_DIR ({}): {}", dir, e);
        }

        let appender = tracing_appender::rolling::weekly(dir, "log");
        let (non_blocking_appender, guard) = tracing_appender::non_blocking(appender);

        file_guard_slot = Some(guard);
        writer = Some(non_blocking_appender);
    }

    let fmt_layer = if let Some(true) = config.json {
        let layer = fmt::layer().json();
        if let Some(writer) = writer {
            layer.with_writer(writer).boxed()
        } else {
            layer.boxed()
        }
    } else {
        let layer = fmt::layer();
        if let Some(writer) = writer {
            layer.with_writer(writer).boxed()
        } else {
            layer.boxed()
        }
    };

    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(ErrorLayer::default())
        .with(fmt_layer);

    if let Err(e) = subscriber.try_init() {
        eprintln!("Failed to initialize tracing: {}", e);
    }

    if let Some(guard) = file_guard_slot {
        let _ = FILE_GUARD.set(guard);
    }
}
