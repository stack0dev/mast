use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::MastError;

/// Advisory file lock using OS-level flock + PID tracking.
///
/// Two layers of protection:
/// 1. `flock(LOCK_EX | LOCK_NB)` on a `.lock` sidecar file — released automatically
///    by the OS when the process dies (even SIGKILL).
/// 2. PID written to the lock file — if flock somehow becomes stale (fd inheritance
///    to a zombie child, NFS quirks, etc.), we can check if the owning process is alive
///    and recover.
pub struct FileLock {
    path: PathBuf,
    #[cfg(unix)]
    _file: File,
}

impl FileLock {
    /// Acquire the lock for the given database path.
    ///
    /// Creates `{db_path}.lock`, acquires an exclusive flock, and writes our PID.
    /// If a stale lock is detected (owner PID is dead), recovers automatically.
    pub fn acquire(db_path: &Path) -> Result<Self, MastError> {
        let lock_path = db_path.with_extension("mast.lock");

        match Self::try_acquire(&lock_path) {
            Ok(lock) => Ok(lock),
            Err(_) => {
                // flock failed — check if the owning process is still alive
                if Self::is_lock_stale(&lock_path) {
                    // Owner is dead. Remove the stale lock file and retry.
                    // This handles cases where flock wasn't properly released
                    // (fd inherited by child process, filesystem quirks, etc.)
                    let _ = fs::remove_file(&lock_path);
                    Self::try_acquire(&lock_path).map_err(|_| {
                        MastError::Storage(
                            "Database already open by another process. \
                             Detected stale lock but failed to recover."
                                .into(),
                        )
                    })
                } else {
                    Err(MastError::Storage(
                        "Database already open by another process".into(),
                    ))
                }
            }
        }
    }

    #[cfg(unix)]
    fn try_acquire(lock_path: &Path) -> Result<Self, MastError> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(|e| MastError::Storage(format!("failed to open lock file: {e}")))?;

        // Non-blocking exclusive lock
        let ret = unsafe { libc::flock(fd_of(&file), libc::LOCK_EX | libc::LOCK_NB) };
        if ret != 0 {
            return Err(MastError::Storage("flock failed".into()));
        }

        // Write our PID
        let pid = std::process::id();
        // Truncate then write — we hold the lock so this is safe
        let _ = file.set_len(0);
        (&file)
            .write_all(pid.to_string().as_bytes())
            .map_err(|e| MastError::Storage(format!("failed to write lock file: {e}")))?;

        Ok(Self {
            path: lock_path.to_path_buf(),
            _file: file,
        })
    }

    #[cfg(not(unix))]
    fn try_acquire(lock_path: &Path) -> Result<Self, MastError> {
        // On non-Unix, fall through to redb's own locking
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(lock_path)
            .map_err(|e| MastError::Storage(format!("failed to open lock file: {e}")))?;

        let pid = std::process::id();
        (&file)
            .write_all(pid.to_string().as_bytes())
            .map_err(|e| MastError::Storage(format!("failed to write lock file: {e}")))?;

        Ok(Self {
            path: lock_path.to_path_buf(),
            _file: file,
        })
    }

    /// Check if the lock file's PID refers to a dead process.
    fn is_lock_stale(lock_path: &Path) -> bool {
        let pid = match Self::read_pid(lock_path) {
            Some(pid) => pid,
            None => {
                // Can't read PID — treat as stale (lock file exists but is corrupt/empty)
                return lock_path.exists();
            }
        };

        // Don't consider our own PID as stale
        if pid == std::process::id() {
            return false;
        }

        !is_pid_alive(pid)
    }

    fn read_pid(lock_path: &Path) -> Option<u32> {
        let mut contents = String::new();
        File::open(lock_path).ok()?.read_to_string(&mut contents).ok()?;
        contents.trim().parse().ok()
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        // Remove the lock file. The flock is released when _file is dropped.
        let _ = fs::remove_file(&self.path);
    }
}

/// Check if a process with the given PID is alive.
#[cfg(unix)]
fn is_pid_alive(pid: u32) -> bool {
    // kill(pid, 0) doesn't send a signal — it just checks if the process exists.
    // Returns 0 if alive, -1 with ESRCH if not.
    let ret = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if ret == 0 {
        return true;
    }
    // EPERM means the process exists but we can't signal it — still alive
    let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
    errno != libc::ESRCH
}

#[cfg(not(unix))]
fn is_pid_alive(_pid: u32) -> bool {
    // Conservative: assume alive on non-Unix
    true
}

#[cfg(unix)]
fn fd_of(file: &File) -> libc::c_int {
    use std::os::unix::io::AsRawFd;
    file.as_raw_fd()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn acquire_and_release() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.mast");
        File::create(&db_path).unwrap();

        let lock = FileLock::acquire(&db_path).unwrap();
        let lock_path = db_path.with_extension("mast.lock");
        assert!(lock_path.exists());

        // PID should be ours
        let pid = FileLock::read_pid(&lock_path).unwrap();
        assert_eq!(pid, std::process::id());

        drop(lock);
        assert!(!lock_path.exists());
    }

    #[test]
    fn double_acquire_fails() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.mast");
        File::create(&db_path).unwrap();

        let _lock1 = FileLock::acquire(&db_path).unwrap();
        let result = FileLock::acquire(&db_path);
        assert!(result.is_err());
    }

    #[test]
    fn stale_lock_recovery() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.mast");
        File::create(&db_path).unwrap();

        // Simulate a stale lock: write a dead PID
        let lock_path = db_path.with_extension("mast.lock");
        fs::write(&lock_path, "99999").unwrap(); // almost certainly dead on a dev machine

        // Should recover and acquire
        let lock = FileLock::acquire(&db_path).unwrap();
        let pid = FileLock::read_pid(&lock_path).unwrap();
        assert_eq!(pid, std::process::id());
        drop(lock);
    }

    #[test]
    fn corrupt_lock_file_recovery() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.mast");
        File::create(&db_path).unwrap();

        // Simulate a corrupt lock file
        let lock_path = db_path.with_extension("mast.lock");
        fs::write(&lock_path, "not-a-pid").unwrap();

        // Should treat as stale and recover
        let lock = FileLock::acquire(&db_path).unwrap();
        assert!(lock_path.exists());
        drop(lock);
    }

    #[cfg(unix)]
    #[test]
    fn is_pid_alive_self() {
        assert!(is_pid_alive(std::process::id()));
    }

    #[cfg(unix)]
    #[test]
    fn is_pid_alive_dead() {
        // PID 99999 is almost certainly not running on a dev machine.
        // Avoid u32::MAX since it wraps to pid_t -1 (broadcast signal).
        assert!(!is_pid_alive(99999));
    }
}
