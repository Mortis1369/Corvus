//! Minimal logger: writes `YYYY-MM-DD HH:MM:SS LEVEL target: message` lines to stderr and,
//! optionally, to a file. A failed write is silently dropped — logging must never be the
//! thing that crashes the launcher on a read-only or vanished drive.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

struct Logger {
    file: Mutex<Option<File>>,
    level: log::LevelFilter,
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {:<5} {}: {}\n",
            timestamp(),
            record.level(),
            record.target(),
            record.args()
        );
        let _ = std::io::stderr().write_all(line.as_bytes());
        if let Ok(mut guard) = self.file.lock() {
            if let Some(f) = guard.as_mut() {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut guard) = self.file.lock() {
            if let Some(f) = guard.as_mut() {
                let _ = f.flush();
            }
        }
    }
}

/// Install the logger. `file` is appended to if it can be opened; if it can't, logging
/// continues on stderr only and the error is reported through the returned `Option`.
/// Calling this twice is harmless (the second call is ignored).
pub fn init(file: Option<&Path>, level: log::LevelFilter) -> Option<std::io::Error> {
    let mut open_err = None;
    let handle = file.and_then(|p| {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match OpenOptions::new().create(true).append(true).open(p) {
            Ok(f) => Some(f),
            Err(e) => {
                open_err = Some(e);
                None
            }
        }
    });
    let logger = Logger {
        file: Mutex::new(handle),
        level,
    };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(level);
    }
    if let Some(e) = &open_err {
        log::warn!(
            "log file {:?} could not be opened: {e}; logging to stderr only",
            file
        );
    }
    open_err
}

/// `YYYY-MM-DD HH:MM:SS` in UTC, without pulling in a date crate.
pub fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    let t = secs % 86_400;
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        t / 3600,
        (t % 3600) / 60,
        t % 60
    )
}

/// Days since 1970-01-01 → (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(20_771), (2026, 11, 14));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn init_with_unwritable_path_does_not_panic() {
        // Only the first init in the process installs; later ones are no-ops. Either way: no panic.
        let err = init(
            Some(Path::new("Z:\\no\\such\\drive\\corvus.log")),
            log::LevelFilter::Info,
        );
        log::info!("still alive");
        // Z: may actually exist on some machines; we only care that nothing exploded.
        let _ = err;
    }

    #[test]
    fn writes_to_file_when_possible() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sub").join("corvus.log");
        // If another test already installed the global logger this one can't; just exercise the
        // open path and the line formatter directly.
        let _ = init(Some(&p), log::LevelFilter::Info);
        let logger = Logger {
            file: Mutex::new(Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&p)
                    .unwrap(),
            )),
            level: log::LevelFilter::Info,
        };
        log::Log::log(
            &logger,
            &log::Record::builder()
                .args(format_args!("hello file"))
                .level(log::Level::Info)
                .target("test")
                .build(),
        );
        log::Log::flush(&logger);
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.contains("INFO  test: hello file"), "{text}");
    }
}
