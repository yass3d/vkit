use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

use crate::session_snapshot::SessionSnapshot;

pub const HEARTBEAT: Duration = Duration::from_secs(10);

pub const STALE_AFTER: Duration = Duration::from_secs(300);

pub const AUTOSAVE_DELAY: Duration = Duration::from_secs(15);

const LOCK_NAME: &str = "session.lock";
const SNAPSHOT_NAME: &str = "session.recovery.json";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockState {
    Free,

    Live,

    Stale,
}

pub const fn classify_lock(age: Option<Duration>) -> LockState {
    match age {
        None => LockState::Free,
        Some(age) if age.as_secs() >= STALE_AFTER.as_secs() => LockState::Stale,
        Some(_) => LockState::Live,
    }
}

enum HolderProbe {
    NoLock,
    Held,
    Orphaned,
    Unknown,
}

#[cfg(windows)]
fn probe_lock_holder(path: &Path) -> HolderProbe {
    match OpenOptions::new().read(true).open(path) {
        Ok(_) => HolderProbe::Orphaned,
        Err(error) if error.kind() == io::ErrorKind::NotFound => HolderProbe::NoLock,
        Err(error)
            if error.raw_os_error()
                == Some(windows_sys::Win32::Foundation::ERROR_SHARING_VIOLATION as i32) =>
        {
            HolderProbe::Held
        }
        Err(_) => HolderProbe::Unknown,
    }
}

#[cfg(not(windows))]
fn probe_lock_holder(path: &Path) -> HolderProbe {
    if path.exists() {
        HolderProbe::Unknown
    } else {
        HolderProbe::NoLock
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AutosaveSchedule {
    pending_since: Option<Duration>,

    last_heartbeat: Option<Duration>,
}

impl AutosaveSchedule {
    pub fn mark_dirty(&mut self, now: Duration) {
        self.pending_since.get_or_insert(now);
    }

    pub fn should_write(&self, now: Duration) -> bool {
        self.pending_since
            .is_some_and(|since| now.saturating_sub(since) >= AUTOSAVE_DELAY)
    }

    pub fn mark_written(&mut self, now: Duration) {
        self.pending_since = None;
        self.last_heartbeat = Some(now);
    }

    pub fn should_heartbeat(&self, now: Duration) -> bool {
        self.last_heartbeat
            .is_none_or(|last| now.saturating_sub(last) >= HEARTBEAT)
    }

    pub fn mark_heartbeat(&mut self, now: Duration) {
        self.last_heartbeat = Some(now);
    }
}

#[derive(Debug)]
pub struct RecoveryStore {
    directory: PathBuf,

    held_lock: Mutex<Option<File>>,
}

impl RecoveryStore {
    pub fn discover() -> Option<Self> {
        let root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
        Some(Self {
            directory: root.join(crate::APP_NAME),
            held_lock: Mutex::new(None),
        })
    }

    #[cfg(test)]
    pub const fn at(directory: PathBuf) -> Self {
        Self {
            directory,
            held_lock: Mutex::new(None),
        }
    }

    fn lock_path(&self) -> PathBuf {
        self.directory.join(LOCK_NAME)
    }

    fn snapshot_path(&self) -> PathBuf {
        self.directory.join(SNAPSHOT_NAME)
    }

    fn held(&self) -> MutexGuard<'_, Option<File>> {
        self.held_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub fn inspect(&self, now: SystemTime) -> LockState {
        let path = self.lock_path();
        match probe_lock_holder(&path) {
            HolderProbe::NoLock => LockState::Free,
            HolderProbe::Held => LockState::Live,
            HolderProbe::Orphaned => LockState::Stale,
            HolderProbe::Unknown => {
                let age = fs::metadata(&path)
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .map(|modified| now.duration_since(modified).unwrap_or_default());
                classify_lock(age)
            }
        }
    }

    pub fn claim(&self) -> io::Result<()> {
        fs::create_dir_all(&self.directory)?;
        self.held().take();
        let mut options = OpenOptions::new();
        options.write(true).create(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt as _;
            options.share_mode(0);
        }
        let mut file = options.open(self.lock_path())?;
        file.set_len(0)?;
        file.write_all(b"vkit")?;
        *self.held() = Some(file);
        Ok(())
    }

    pub fn heartbeat(&self) -> io::Result<()> {
        let mut guard = self.held();
        let file = guard
            .as_mut()
            .ok_or_else(|| io::Error::other("the session lock is not held"))?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(b"vkit")
    }

    pub fn save(&self, snapshot: &SessionSnapshot) -> io::Result<()> {
        fs::create_dir_all(&self.directory)?;
        let bytes = serde_json::to_vec(snapshot).map_err(io::Error::other)?;
        let temporary = self
            .directory
            .join(format!(".{SNAPSHOT_NAME}.{}.tmp", std::process::id()));
        fs::write(&temporary, &bytes)?;
        match fs::rename(&temporary, self.snapshot_path()) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                Err(error)
            }
        }
    }

    pub fn load(&self) -> Option<SessionSnapshot> {
        let bytes = fs::read(self.snapshot_path()).ok()?;
        let snapshot: SessionSnapshot = serde_json::from_slice(&bytes).ok()?;
        (snapshot.is_readable() && snapshot.worth_offering()).then_some(snapshot)
    }

    pub fn release(&self) {
        self.held().take();
        let _ = fs::remove_file(self.lock_path());
        let _ = fs::remove_file(self.snapshot_path());
    }

    pub fn release_snapshot(&self) {
        let _ = fs::remove_file(self.snapshot_path());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_snapshot::{SNAPSHOT_VERSION, SparseDisplacement};

    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "vkit-recovery-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn worked_on() -> SessionSnapshot {
        SessionSnapshot {
            version: SNAPSHOT_VERSION,
            sculpt: SparseDisplacement::from_dense(&[[1.0, 0.0, 0.0]]),
            ..SessionSnapshot::default()
        }
    }

    #[test]
    fn a_lock_is_free_live_or_stale_by_how_cold_it_is() {
        assert_eq!(classify_lock(None), LockState::Free);
        assert_eq!(classify_lock(Some(Duration::ZERO)), LockState::Live);
        assert_eq!(classify_lock(Some(HEARTBEAT)), LockState::Live);
        assert_eq!(classify_lock(Some(HEARTBEAT * 3)), LockState::Live);
        assert_eq!(classify_lock(Some(STALE_AFTER)), LockState::Stale);
        assert_eq!(classify_lock(Some(STALE_AFTER * 100)), LockState::Stale);
    }

    #[cfg(windows)]
    #[test]
    fn a_running_session_reads_as_live_and_not_as_a_crash() {
        let directory = scratch("live");
        let store = RecoveryStore::at(directory.clone());
        assert_eq!(store.inspect(SystemTime::now()), LockState::Free);
        store.claim().unwrap();

        let second = RecoveryStore::at(directory);
        assert_eq!(second.inspect(SystemTime::now()), LockState::Live);
        assert!(second.claim().is_err(), "the lock is not shareable");

        store.release();
        assert_eq!(store.inspect(SystemTime::now()), LockState::Free);
    }

    #[cfg(windows)]
    #[test]
    fn a_crash_reads_as_stale_immediately_rather_than_after_a_cooldown() {
        let directory = scratch("stale");
        let crashed = RecoveryStore::at(directory.clone());
        crashed.claim().unwrap();
        drop(crashed);

        let relaunched = RecoveryStore::at(directory);
        assert_eq!(
            relaunched.inspect(SystemTime::now()),
            LockState::Stale,
            "the lock file is seconds old, but its owner is gone"
        );
        relaunched.claim().unwrap();
        assert_eq!(relaunched.inspect(SystemTime::now()), LockState::Live);
        relaunched.release();
    }

    #[test]
    fn a_snapshot_survives_a_save_and_a_load() {
        let store = RecoveryStore::at(scratch("round-trip"));
        assert!(store.load().is_none());
        let snapshot = worked_on();
        store.save(&snapshot).unwrap();
        assert_eq!(store.load(), Some(snapshot));
        store.release();
        assert!(store.load().is_none());
    }

    #[test]
    fn saving_twice_leaves_no_temporary_behind() {
        let directory = scratch("atomic");
        let store = RecoveryStore::at(directory.clone());
        store.save(&worked_on()).unwrap();
        store.save(&worked_on()).unwrap();
        let strays: Vec<_> = fs::read_dir(&directory)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(strays.is_empty(), "left {strays:?}");
    }

    #[test]
    fn an_unusable_snapshot_offers_nothing_rather_than_failing() {
        let directory = scratch("unusable");
        let store = RecoveryStore::at(directory.clone());
        let path = directory.join(SNAPSHOT_NAME);
        for contents in [
            "not json at all".to_owned(),
            r#"{"version": 9999}"#.to_owned(),
            format!(r#"{{"version": {SNAPSHOT_VERSION}}}"#),
        ] {
            fs::write(&path, contents.as_bytes()).unwrap();
            assert!(store.load().is_none(), "accepted {contents}");
        }
    }

    #[test]
    fn a_continuous_drag_writes_once_rather_than_once_per_frame() {
        let mut schedule = AutosaveSchedule::default();
        let mut writes = 0;

        const FRAMES: u64 = 60 * 120;
        for frame in 0..FRAMES {
            let now = Duration::from_millis(frame * 16);
            schedule.mark_dirty(now);
            if schedule.should_write(now) {
                writes += 1;
                schedule.mark_written(now);
            }
        }

        let elapsed = Duration::from_millis(FRAMES * 16);
        let expected = elapsed.as_secs() / AUTOSAVE_DELAY.as_secs();
        assert!(
            (expected.saturating_sub(1)..=expected).contains(&writes),
            "{writes} writes in {elapsed:?} at a {AUTOSAVE_DELAY:?} delay"
        );
    }

    #[test]
    fn an_idle_session_never_writes() {
        let schedule = AutosaveSchedule::default();
        for minute in 0..60_u64 {
            assert!(!schedule.should_write(Duration::from_secs(minute * 60)));
        }
    }

    #[test]
    fn the_heartbeat_runs_even_when_nothing_is_dirty() {
        let mut schedule = AutosaveSchedule::default();
        assert!(schedule.should_heartbeat(Duration::ZERO));
        schedule.mark_heartbeat(Duration::ZERO);
        assert!(!schedule.should_heartbeat(HEARTBEAT / 2));
        assert!(schedule.should_heartbeat(HEARTBEAT));
    }

    #[test]
    fn a_failed_write_is_not_retried_until_something_changes_again() {
        let mut schedule = AutosaveSchedule::default();
        schedule.mark_dirty(Duration::ZERO);
        let due = AUTOSAVE_DELAY;
        assert!(schedule.should_write(due));
        schedule.mark_written(due);
        assert!(!schedule.should_write(due + AUTOSAVE_DELAY * 10));
        schedule.mark_dirty(due + Duration::from_secs(1));
        assert!(schedule.should_write(due + Duration::from_secs(1) + AUTOSAVE_DELAY));
    }
}
