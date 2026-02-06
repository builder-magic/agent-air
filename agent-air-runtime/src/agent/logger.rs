// Logger for LLM agents using tracing

use std::fs::{self, File};
use std::io;
use std::path::Path;

use chrono::Local;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

const LOG_DIR: &str = "logs";

/// Tracing-based logger that writes to daily log files.
///
/// Holds a worker guard to ensure logs are flushed before shutdown.
pub struct Logger {
    _guard: WorkerGuard,
}

impl Logger {
    /// Create a new logger with a custom log file prefix.
    ///
    /// # Arguments
    /// * `prefix` - The prefix for log files (e.g., "multi_code", "europa")
    ///
    /// Log files will be created as `logs/{prefix}-{date}.log`
    pub fn new(prefix: &str) -> io::Result<Self> {
        let log_dir = Path::new(LOG_DIR);
        if !log_dir.exists() {
            fs::create_dir_all(log_dir)?;
        }

        let date = Local::now().format("%Y-%m-%d");
        let log_file_name = format!("{}/{}-{}.log", LOG_DIR, prefix, date);

        let file = File::create(&log_file_name)?;

        let (non_blocking, guard) = tracing_appender::non_blocking(file);

        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));

        tracing_subscriber::registry()
            .with(env_filter)
            .with(
                fmt::layer()
                    .with_writer(non_blocking)
                    .with_ansi(false)
                    .with_target(true)
                    .with_thread_ids(false)
                    .with_file(true)
                    .with_line_number(true),
            )
            .init();

        tracing::info!("Logger initialized, writing to {}", log_file_name);

        Ok(Self { _guard: guard })
    }
}
