use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use crate::bus::RuntimeEventEnvelope;

pub const RUNTIME_LOG_FILE_NAME: &str = "runtime-events.jsonl";
pub const DEFAULT_RUNTIME_LOG_MAX_BYTES: u64 = 10 * 1024 * 1024;
pub const DEFAULT_RUNTIME_LOG_RETAINED_FILES: usize = 5;

#[derive(Debug, Clone)]
pub struct RuntimeLogConfig {
    pub directory: PathBuf,
    pub max_bytes: u64,
    pub retained_files: usize,
}

impl RuntimeLogConfig {
    pub fn default_for_app() -> Self {
        Self {
            directory: default_log_directory(),
            max_bytes: DEFAULT_RUNTIME_LOG_MAX_BYTES,
            retained_files: DEFAULT_RUNTIME_LOG_RETAINED_FILES,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeJsonlLogWriter {
    config: RuntimeLogConfig,
}

impl RuntimeJsonlLogWriter {
    pub fn new(config: RuntimeLogConfig) -> io::Result<Self> {
        fs::create_dir_all(&config.directory)?;
        Ok(Self { config })
    }

    pub fn config(&self) -> &RuntimeLogConfig {
        &self.config
    }

    pub fn append(&self, event: &RuntimeEventEnvelope) -> io::Result<()> {
        fs::create_dir_all(&self.config.directory)?;
        let mut line = serde_json::to_vec(event).map_err(io::Error::other)?;
        line.push(b'\n');

        if self.should_rotate(line.len() as u64)? {
            self.rotate()?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.active_path())?;
        file.write_all(&line)
    }

    pub fn active_path(&self) -> PathBuf {
        self.config.directory.join(RUNTIME_LOG_FILE_NAME)
    }

    fn should_rotate(&self, incoming_bytes: u64) -> io::Result<bool> {
        let active = self.active_path();
        if !active.exists() {
            return Ok(false);
        }
        let current_size = active.metadata()?.len();
        Ok(current_size > 0 && current_size + incoming_bytes > self.config.max_bytes)
    }

    fn rotate(&self) -> io::Result<()> {
        if self.config.retained_files == 0 {
            let active = self.active_path();
            if active.exists() {
                fs::remove_file(active)?;
            }
            return Ok(());
        }

        let oldest = rotated_path(&self.config.directory, self.config.retained_files);
        if oldest.exists() {
            fs::remove_file(oldest)?;
        }

        for index in (1..self.config.retained_files).rev() {
            let from = rotated_path(&self.config.directory, index);
            let to = rotated_path(&self.config.directory, index + 1);
            if from.exists() {
                fs::rename(from, to)?;
            }
        }

        let active = self.active_path();
        if active.exists() {
            fs::rename(active, rotated_path(&self.config.directory, 1))?;
        }

        Ok(())
    }
}

pub fn default_log_directory() -> PathBuf {
    if let Ok(app_data) = std::env::var("HAM_PLATFORM_LOG_DIR") {
        return PathBuf::from(app_data);
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(local_app_data)
                .join("KE8YGW Logger")
                .join("logs");
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Logs")
                .join("KE8YGW Logger");
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("ke8ygw-logger")
            .join("logs");
    }

    std::env::temp_dir().join("ke8ygw-logger").join("logs")
}

fn rotated_path(directory: &Path, index: usize) -> PathBuf {
    directory.join(format!("{RUNTIME_LOG_FILE_NAME}.{index}"))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use serde_json::json;
    use uuid::Uuid;

    use crate::bus::{RuntimeEventEnvelope, RuntimeEventSeverity};

    use super::{RuntimeJsonlLogWriter, RuntimeLogConfig, RUNTIME_LOG_FILE_NAME};

    #[test]
    fn runtime_log_writer_rotates_and_retains_configured_files() {
        let dir = unique_temp_dir();
        let writer = RuntimeJsonlLogWriter::new(RuntimeLogConfig {
            directory: dir.clone(),
            max_bytes: 420,
            retained_files: 2,
        })
        .unwrap();

        for index in 0..8 {
            let event = RuntimeEventEnvelope::new(
                "diagnostics.rotation",
                RuntimeEventSeverity::Info,
                "test",
                None,
                Uuid::new_v4(),
                Uuid::new_v4(),
                Uuid::new_v4(),
                None,
                format!("rotation event {index}"),
                Some(json!({"index": index, "password": "not-written"})),
                None,
            );
            writer.append(&event).unwrap();
        }

        assert!(dir.join(RUNTIME_LOG_FILE_NAME).exists());
        assert!(dir.join(format!("{RUNTIME_LOG_FILE_NAME}.1")).exists());
        assert!(dir.join(format!("{RUNTIME_LOG_FILE_NAME}.2")).exists());
        assert!(!dir.join(format!("{RUNTIME_LOG_FILE_NAME}.3")).exists());

        fs::remove_dir_all(dir).unwrap();
    }

    fn unique_temp_dir() -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ham-core-runtime-log-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
