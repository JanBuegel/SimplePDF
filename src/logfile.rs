//! Log file for diagnosing problems on presentation PCs, where release builds have no console.
//!
//! Windows: `%LOCALAPPDATA%\SimplePDF\simplepdf.log`, elsewhere `simplepdf.log` in the temp
//! folder. Each start appends to it. Panics are logged too.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use log::{Level, LevelFilter, Log, Metadata, Record};

struct FileLog {
    file: Mutex<File>,
    start: Instant,
}

impl Log for FileLog {
    fn enabled(&self, meta: &Metadata) -> bool {
        // Everything of our own; only warnings and errors from wgpu, winit and the rest.
        meta.level() <= Level::Warn || meta.target().starts_with("simplepdf")
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let t = self.start.elapsed().as_secs_f64();
        if let Ok(mut f) = self.file.lock() {
            let _ = writeln!(
                f,
                "{t:9.3} {:5} {}: {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }

    fn flush(&self) {
        if let Ok(mut f) = self.file.lock() {
            let _ = f.flush();
        }
    }
}

pub fn path() -> PathBuf {
    match std::env::var_os("LOCALAPPDATA") {
        Some(dir) if cfg!(windows) => PathBuf::from(dir).join("SimplePDF").join("simplepdf.log"),
        _ => std::env::temp_dir().join("simplepdf.log"),
    }
}

/// Starts logging; without a writable log file the app runs without one.
pub fn init() {
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Appends, so earlier runs stay readable; starts over once the file gets large.
    let large = std::fs::metadata(&path).is_ok_and(|m| m.len() > 4 << 20);
    let file = if large {
        File::create(&path)
    } else {
        OpenOptions::new().create(true).append(true).open(&path)
    };
    let Ok(file) = file else {
        return;
    };
    let logger = FileLog {
        file: Mutex::new(file),
        start: Instant::now(),
    };
    if log::set_boxed_logger(Box::new(logger)).is_err() {
        return;
    }
    log::set_max_level(LevelFilter::Debug);

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("PANIC: {info}");
        log::logger().flush();
        default_hook(info);
    }));
}
