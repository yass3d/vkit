use std::any::Any;
use std::env;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
#[cfg(any(not(windows), test))]
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
pub const LOG_GENERATIONS: usize = 3;

pub const LOG_FILE_NAME: &str = "vkit.log";
pub const CRASH_REPORT_FILE_NAME: &str = "crash.log";

pub const PREVIOUS_LOG_FILE_NAME: &str = "vkit.log.1";

static GLOBAL_LOG: OnceLock<DiagnosticLog> = OnceLock::new();
static PANIC_HOOK_INSTALLED: OnceLock<()> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Debug,
    Info,
    Warning,
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warning => "WARN",
            Self::Error => "ERROR",
        })
    }
}

pub struct DiagnosticLog {
    path: PathBuf,
    maximum_bytes: u64,
    generations: usize,
    writer: Mutex<LogWriter>,
}

struct LogWriter {
    file: Option<File>,
    length: u64,
}

impl DiagnosticLog {
    pub fn at(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::at_with_policy(path.as_ref(), MAX_LOG_BYTES, LOG_GENERATIONS)
    }

    fn at_with_policy(path: &Path, maximum_bytes: u64, generations: usize) -> io::Result<Self> {
        validate_policy(path, maximum_bytes, generations)?;
        create_parent_directory(path)?;

        let existing_length = file_length(path)?;
        if existing_length >= maximum_bytes && existing_length > 0 {
            rotate_files(path, generations)?;
        }
        let (file, length) = open_append(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            maximum_bytes,
            generations,
            writer: Mutex::new(LogWriter {
                file: Some(file),
                length,
            }),
        })
    }

    pub fn record(
        &self,
        severity: Severity,
        component: &str,
        event: &str,
        message: &str,
    ) -> io::Result<()> {
        if component.trim().is_empty() {
            return Err(invalid_input("diagnostic component must not be empty"));
        }
        if event.trim().is_empty() {
            return Err(invalid_input("diagnostic event must not be empty"));
        }

        let line = format!(
            "{}\t{}\t{}\t{}\t{}\n",
            record_timestamp(),
            severity,
            sanitize_field(component),
            sanitize_field(event),
            sanitize_field(message),
        );
        let line_length = u64::try_from(line.len())
            .map_err(|_| invalid_input("diagnostic record length exceeds u64"))?;
        let mut writer = self.lock_writer();
        if writer.length > 0 && writer.length.saturating_add(line_length) > self.maximum_bytes {
            self.rotate_locked(&mut writer)?;
        }
        let file = writer
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("diagnostic file is not open"))?;
        file.write_all(line.as_bytes())?;
        writer.length = writer.length.saturating_add(line_length);
        Ok(())
    }

    pub fn flush(&self) -> io::Result<()> {
        let mut writer = self.lock_writer();
        let file = writer
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("diagnostic file is not open"))?;
        file.flush()
    }

    pub fn start_session(&self) -> io::Result<()> {
        let mut writer = self.lock_writer();
        if writer.length == 0 {
            return Ok(());
        }
        self.rotate_locked(&mut writer)
    }

    pub fn record_startup(&self, version: &str) -> io::Result<()> {
        let executable = env::current_exe()
            .ok()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "<unavailable>".to_owned());
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        self.record(
            Severity::Info,
            "app",
            "startup",
            &format!(
                "version={version}; profile={profile}; platform={}-{}; executable={executable}",
                env::consts::OS,
                env::consts::ARCH
            ),
        )?;
        self.flush()
    }

    fn lock_writer(&self) -> MutexGuard<'_, LogWriter> {
        self.writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn rotate_locked(&self, writer: &mut LogWriter) -> io::Result<()> {
        if let Some(file) = writer.file.as_mut() {
            file.flush()?;
        }
        writer.file.take();

        let rotation_result = rotate_files(&self.path, self.generations);
        match open_append(&self.path) {
            Ok((file, length)) => {
                writer.file = Some(file);
                writer.length = length;
                rotation_result
            }
            Err(open_error) => Err(open_error),
        }
    }
}

pub fn default_log_path() -> io::Result<PathBuf> {
    Ok(log_directory()?.join(LOG_FILE_NAME))
}

pub fn log_directory() -> io::Result<PathBuf> {
    let local_app_data = env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is not set"))?;
    Ok(PathBuf::from(local_app_data)
        .join(crate::APP_NAME)
        .join("logs"))
}

pub fn global_log() -> io::Result<&'static DiagnosticLog> {
    if let Some(log) = GLOBAL_LOG.get() {
        return Ok(log);
    }

    let candidate = DiagnosticLog::at(default_log_path()?)?;
    let _ = GLOBAL_LOG.set(candidate);
    GLOBAL_LOG
        .get()
        .ok_or_else(|| io::Error::other("global diagnostic logger initialization failed"))
}

pub fn initialize_global(version: &str) -> io::Result<&'static DiagnosticLog> {
    let log = global_log()?;
    log.start_session()?;
    log.record_startup(version)?;
    Ok(log)
}

pub fn record(severity: Severity, component: &str, event: &str, message: &str) -> io::Result<()> {
    global_log()?.record(severity, component, event, message)
}

pub fn flush() -> io::Result<()> {
    global_log()?.flush()
}

pub fn read_recent(maximum_bytes: usize) -> io::Result<String> {
    global_log()?.flush()?;
    read_recent_from_path(&default_log_path()?, maximum_bytes)
}

fn read_recent_from_path(path: &Path, maximum_bytes: usize) -> io::Result<String> {
    if maximum_bytes == 0 {
        return Ok(String::new());
    }
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    let maximum_bytes = u64::try_from(maximum_bytes).unwrap_or(u64::MAX);
    let start = length.saturating_sub(maximum_bytes);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity((length - start).min(maximum_bytes) as usize);
    file.read_to_end(&mut bytes)?;
    if start > 0
        && let Some(newline) = bytes.iter().position(|byte| *byte == b'\n')
    {
        bytes.drain(..=newline);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn install_panic_hook() -> io::Result<()> {
    let log = global_log()?;
    PANIC_HOOK_INSTALLED.get_or_init(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |information| {
            let message = panic_message(
                information.payload(),
                information.location().map(|site| PanicSite {
                    file: site.file(),
                    line: site.line(),
                    column: site.column(),
                }),
            );
            let _ = log.record(Severity::Error, "runtime", "panic", &message);
            let _ = log.flush();
            write_crash_report(&message);
            previous(information);
        }));
    });
    Ok(())
}

fn write_crash_report(message: &str) {
    let Ok(directory) = log_directory() else {
        return;
    };
    write_crash_report_in(&directory, message);
}

fn write_crash_report_in(directory: &Path, message: &str) {
    let backtrace = std::backtrace::Backtrace::force_capture();
    let report = format!(
        "{}	{}

{backtrace}
",
        record_timestamp(),
        message
    );
    let _ = std::fs::create_dir_all(directory);

    let path = directory.join(CRASH_REPORT_FILE_NAME);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write as _;
        let _ = file.write_all(report.as_bytes());
        let _ = file.sync_all();
    }
}

fn validate_policy(path: &Path, maximum_bytes: u64, generations: usize) -> io::Result<()> {
    if path.as_os_str().is_empty() || path.file_name().is_none() {
        return Err(invalid_input("diagnostic log path must name a file"));
    }
    if maximum_bytes == 0 {
        return Err(invalid_input("diagnostic rotation size must be positive"));
    }
    if generations == 0 {
        return Err(invalid_input(
            "diagnostic rotation generation count must be positive",
        ));
    }
    Ok(())
}

fn create_parent_directory(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn open_append(path: &Path) -> io::Result<(File, u64)> {
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    let length = file.metadata()?.len();
    Ok((file, length))
}

fn file_length(path: &Path) -> io::Result<u64> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}

fn rotate_files(path: &Path, generations: usize) -> io::Result<()> {
    let oldest = backup_path(path, generations);
    remove_if_present(&oldest)?;

    for generation in (1..generations).rev() {
        let source = backup_path(path, generation);
        if source.exists() {
            fs::rename(source, backup_path(path, generation + 1))?;
        }
    }
    if path.exists() {
        fs::rename(path, backup_path(path, 1))?;
    }
    Ok(())
}

fn backup_path(path: &Path, generation: usize) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(format!(".{generation}"));
    PathBuf::from(value)
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn sanitize_field(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\r' => sanitized.push_str("\\r"),
            '\n' => sanitized.push_str("\\n"),
            '\t' => sanitized.push_str("\\t"),
            '\0' => sanitized.push_str("\\0"),
            value => sanitized.push(value),
        }
    }
    sanitized
}

#[derive(Clone, Copy)]
struct PanicSite<'a> {
    file: &'a str,
    line: u32,
    column: u32,
}

fn panic_message(payload: &(dyn Any + Send), site: Option<PanicSite<'_>>) -> String {
    let payload = if let Some(value) = payload.downcast_ref::<&str>() {
        (*value).to_owned()
    } else if let Some(value) = payload.downcast_ref::<String>() {
        value.clone()
    } else {
        "<non-string panic payload>".to_owned()
    };
    let location = site.map_or_else(
        || "<unknown>".to_owned(),
        |site| format!("{}:{}:{}", site.file, site.line, site.column),
    );
    format!(
        "location={}; payload={}",
        sanitize_field(&location),
        sanitize_field(&payload)
    )
}

fn record_timestamp() -> String {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::SYSTEMTIME;
        use windows_sys::Win32::System::SystemInformation::GetLocalTime;

        let mut local = SYSTEMTIME {
            wYear: 0,
            wMonth: 0,
            wDayOfWeek: 0,
            wDay: 0,
            wHour: 0,
            wMinute: 0,
            wSecond: 0,
            wMilliseconds: 0,
        };

        #[allow(
            unsafe_code,
            reason = "std cannot read local time; this is the one call that can"
        )]
        unsafe {
            GetLocalTime(&raw mut local);
        }

        #[allow(clippy::needless_return, reason = "cfg-selected early return")]
        return format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}",
            local.wYear,
            local.wMonth,
            local.wDay,
            local.wHour,
            local.wMinute,
            local.wSecond,
            local.wMilliseconds
        );
    }
    #[cfg(not(windows))]
    utc_timestamp(SystemTime::now())
}

#[cfg(any(not(windows), test))]
fn utc_timestamp(now: SystemTime) -> String {
    let duration = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = i64::try_from(duration.as_secs()).unwrap_or(i64::MAX);
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_date(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{:03}Z",
        duration.subsec_millis()
    )
}

#[cfg(any(not(windows), test))]
fn civil_date(days_since_epoch: i64) -> (i64, i64, i64) {
    let shifted = days_since_epoch + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

fn invalid_input(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::fs;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use std::time::Duration;

    static TEST_DIRECTORY_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn a_crash_report_lands_and_accumulates() {
        let directory = TestDirectory::new();
        write_crash_report_in(&directory.0, "first: something went wrong");
        write_crash_report_in(&directory.0, "second: and again");

        let text = fs::read_to_string(directory.0.join(CRASH_REPORT_FILE_NAME))
            .expect("the crash report exists");
        assert!(text.contains("first: something went wrong"), "{text}");
        assert!(text.contains("second: and again"), "{text}");

        assert!(text.len() > 200, "no backtrace captured: {text}");
    }

    #[test]
    fn the_log_and_its_crash_report_share_one_named_folder() {
        let (Ok(log), Ok(directory)) = (default_log_path(), log_directory()) else {
            assert!(default_log_path().is_err() && log_directory().is_err());
            return;
        };
        assert_eq!(log.parent(), Some(directory.as_path()));
        assert!(log.ends_with(LOG_FILE_NAME));

        let staged = TestDirectory::new();
        write_crash_report_in(&staged.0, "a fault the reporter has to be able to find");
        assert!(staged.0.join(CRASH_REPORT_FILE_NAME).is_file());
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let id = TEST_DIRECTORY_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                env::temp_dir().join(format!("vkit-diagnostics-test-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("create isolated test directory");
            Self(path)
        }

        fn log_path(&self) -> PathBuf {
            self.0.join("nested").join("vkit.log")
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let expected_prefix = format!("vkit-diagnostics-test-{}-", std::process::id());
            let is_owned_test_path = self
                .0
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.starts_with(&expected_prefix));
            if is_owned_test_path {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn each_session_starts_a_fresh_log_and_keeps_the_last_one() {
        let directory = TestDirectory::new();
        let path = directory.log_path();
        let first = DiagnosticLog::at(&path).expect("open first session");
        first
            .record(Severity::Info, "app", "first", "previous run")
            .expect("append");
        first.flush().expect("flush");

        let second = DiagnosticLog::at(&path).expect("reopen");
        second.start_session().expect("start a session");
        second
            .record(Severity::Info, "app", "second", "current run")
            .expect("append");
        second.flush().expect("flush");

        let current = fs::read_to_string(&path).expect("read current log");
        assert!(current.contains("current run"));
        assert!(
            !current.contains("previous run"),
            "the previous run must not be mixed into this one"
        );

        let previous =
            fs::read_to_string(backup_path(&path, 1)).expect("read the retained previous run");
        assert!(previous.contains("previous run"));

        let third = DiagnosticLog::at(&path).expect("reopen");
        let fresh = DiagnosticLog::at(directory.0.join("empty.log")).expect("open empty");
        fresh.start_session().expect("no-op on an empty file");
        assert!(!backup_path(&directory.0.join("empty.log"), 1).exists());
        drop(third);
    }

    #[test]
    fn append_survives_reopening_and_creates_parent_directories() {
        let directory = TestDirectory::new();
        let path = directory.log_path();
        DiagnosticLog::at(&path)
            .expect("open first logger")
            .record(Severity::Info, "app", "first", "하나")
            .expect("append first record");
        let second = DiagnosticLog::at(&path).expect("reopen logger");
        second
            .record(Severity::Warning, "app", "second", "둘")
            .expect("append second record");
        second.flush().expect("flush records");

        let text = fs::read_to_string(path).expect("read UTF-8 log");
        assert!(text.contains("\tINFO\tapp\tfirst\t하나\n"));
        assert!(text.contains("\tWARN\tapp\tsecond\t둘\n"));
    }

    #[test]
    fn the_startup_line_names_the_build_without_naming_the_machine() {
        let directory = TestDirectory::new();
        let path = directory.log_path();
        let log = DiagnosticLog::at(&path).expect("open logger");
        log.record_startup("9.8.7").expect("write startup record");

        let text = fs::read_to_string(path).expect("read startup record");
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let name = env::current_exe()
            .expect("test executable path")
            .file_name()
            .expect("test executable file name")
            .to_string_lossy()
            .into_owned();
        assert!(
            text.contains(&format!(
                "\tINFO\tapp\tstartup\tversion=9.8.7; profile={profile}; platform={}-{}; executable={name}",
                env::consts::OS,
                env::consts::ARCH,
            )),
            "{text}"
        );

        let startup = text
            .lines()
            .find(|line| line.contains("\tapp\tstartup\t"))
            .expect("a startup line");
        for separator in ['\\', '/'] {
            assert!(
                !startup.contains(separator),
                "the startup line carries {separator:?}, so it is carrying a path: {startup}"
            );
        }
    }

    #[test]
    fn carriage_returns_newlines_and_tabs_cannot_inject_records() {
        let directory = TestDirectory::new();
        let path = directory.log_path();
        let log = DiagnosticLog::at(&path).expect("open logger");
        log.record(
            Severity::Error,
            "core\nworker",
            "fit\rfailed",
            "first\r\nsecond\nthird\ttab",
        )
        .expect("write sanitized record");
        log.flush().expect("flush record");

        let text = fs::read_to_string(path).expect("read log");
        assert_eq!(text.lines().count(), 1);
        assert!(text.contains("core\\nworker"));
        assert!(text.contains("fit\\rfailed"));
        assert!(text.contains("first\\r\\nsecond\\nthird\\ttab"));
    }

    #[test]
    fn recent_reader_drops_a_partial_first_record() {
        let directory = TestDirectory::new();
        let path = directory.log_path();
        fs::create_dir_all(path.parent().unwrap()).expect("create log parent");
        fs::write(&path, "first-record\nsecond-record\nthird-record\n").expect("write test log");

        let text = read_recent_from_path(&path, 28).expect("read bounded tail");
        assert_eq!(text, "second-record\nthird-record\n");
        assert_eq!(read_recent_from_path(&path, 0).unwrap(), "");
    }

    #[test]
    fn rotation_keeps_exactly_three_generations() {
        let directory = TestDirectory::new();
        let path = directory.log_path();
        let log = DiagnosticLog::at_with_policy(&path, 160, 3).expect("open small test log");
        for index in 0..8 {
            log.record(
                Severity::Info,
                "rotation",
                "record",
                &format!("marker-{index}-{}", "x".repeat(96)),
            )
            .expect("append rotating record");
        }
        log.flush().expect("flush current generation");

        assert!(path.exists());
        for generation in 1..=3 {
            let backup = backup_path(&path, generation);
            assert!(backup.exists(), "missing generation {generation}");
            assert!(fs::metadata(backup).expect("backup metadata").len() > 0);
        }
        assert!(!backup_path(&path, 4).exists());
    }

    #[test]
    fn the_previous_run_is_named_the_way_rotation_actually_names_it() {
        let rotated = backup_path(Path::new(LOG_FILE_NAME), 1);
        assert_eq!(rotated, Path::new(PREVIOUS_LOG_FILE_NAME));
    }

    #[test]
    fn concurrent_records_remain_complete_unique_lines() {
        let directory = TestDirectory::new();
        let path = directory.log_path();
        let log = Arc::new(DiagnosticLog::at(&path).expect("open logger"));
        let mut workers = Vec::new();
        for worker in 0..8 {
            let log = Arc::clone(&log);
            workers.push(thread::spawn(move || {
                for record in 0..100 {
                    log.record(
                        Severity::Debug,
                        "thread",
                        "record",
                        &format!("worker={worker}; record={record}"),
                    )
                    .expect("append concurrent record");
                }
            }));
        }
        for worker in workers {
            worker.join().expect("worker must not panic");
        }
        log.flush().expect("flush concurrent records");

        let text = fs::read_to_string(path).expect("read log");
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 800);
        let messages: BTreeSet<_> = lines
            .iter()
            .map(|line| line.rsplit('\t').next().expect("message field"))
            .collect();
        assert_eq!(messages.len(), 800);
        assert!(lines.iter().all(|line| line.matches('\t').count() == 4));
    }

    #[test]
    fn panic_payload_formatting_is_tested_without_installing_a_global_hook() {
        let string_payload = String::from("failed\r\ncleanly");
        let message = panic_message(
            &string_payload,
            Some(PanicSite {
                file: "runtime.rs",
                line: 42,
                column: 7,
            }),
        );
        assert_eq!(
            message,
            "location=runtime.rs:42:7; payload=failed\\r\\ncleanly"
        );

        let non_string: Box<dyn Any + Send> = Box::new(17_u32);
        assert_eq!(
            panic_message(non_string.as_ref(), None),
            "location=<unknown>; payload=<non-string panic payload>"
        );
    }

    #[test]
    fn utc_timestamp_has_a_stable_epoch_reference() {
        let value = utc_timestamp(UNIX_EPOCH + Duration::from_millis(123));
        assert_eq!(value, "1970-01-01T00:00:00.123Z");
    }
}
