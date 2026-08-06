use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::session_snapshot::SessionSnapshot;

pub const HEARTBEAT: Duration = Duration::from_secs(10);

pub const STALE_AFTER: Duration = Duration::from_secs(60);

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

#[derive(Clone, Debug)]
pub struct RecoveryStore {
    directory: PathBuf,
}

impl RecoveryStore {
    pub fn discover() -> Option<Self> {
        let root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
        Some(Self {
            directory: root.join(crate::APP_NAME),
        })
    }

    #[cfg(test)]
    pub const fn at(directory: PathBuf) -> Self {
        Self { directory }
    }

    fn lock_path(&self) -> PathBuf {
        self.directory.join(LOCK_NAME)
    }

    fn snapshot_path(&self) -> PathBuf {
        self.directory.join(SNAPSHOT_NAME)
    }

    pub fn inspect(&self, now: SystemTime) -> LockState {
        let age = fs::metadata(self.lock_path())
            .and_then(|metadata| metadata.modified())
            .ok()
            .map(|modified| now.duration_since(modified).unwrap_or_default());
        classify_lock(age)
    }

    pub fn claim(&self) -> io::Result<()> {
        fs::create_dir_all(&self.directory)?;
        fs::write(self.lock_path(), b"vkit")
    }

    pub fn heartbeat(&self) -> io::Result<()> {
        fs::write(self.lock_path(), b"vkit")
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
        (snapshot.is_readable() && snapshot.has_work()).then_some(snapshot)
    }

    pub fn release(&self) {
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

    #[test]
    fn a_running_session_reads_as_live_and_not_as_a_crash() {
        let store = RecoveryStore::at(scratch("live"));
        assert_eq!(store.inspect(SystemTime::now()), LockState::Free);
        store.claim().unwrap();
        assert_eq!(store.inspect(SystemTime::now()), LockState::Live);
        store.release();
        assert_eq!(store.inspect(SystemTime::now()), LockState::Free);
    }

    #[test]
    fn a_session_that_never_released_its_lock_reads_as_stale() {
        let store = RecoveryStore::at(scratch("stale"));
        store.claim().unwrap();
        let later = SystemTime::now() + STALE_AFTER + Duration::from_secs(5);
        assert_eq!(store.inspect(later), LockState::Stale);
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
