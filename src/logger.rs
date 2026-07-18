use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write;
use tracing::error;

pub fn log_transcript(text: &str, file_path: &str) {
    if text.trim().is_empty() {
        return;
    }

    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S.%3f").to_string();
    let log_line = format!("[{}] {}", timestamp, text);

    println!("{}", log_line);

    let mut file = match OpenOptions::new().create(true).append(true).open(file_path) {
        Ok(f) => f,
        Err(e) => {
            error!("Failed to open log file {}: {}", file_path, e);
            return;
        }
    };

    if let Err(e) = writeln!(file, "{}", log_line) {
        error!("Failed to write to log file: {}", e);
    }
}
