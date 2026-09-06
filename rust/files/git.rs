//! The single bounded Git invocation used by tracked file selection.

use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use thiserror::Error;

/// Failure to obtain a complete tracked Markdown list.
#[derive(Debug, Error)]
pub enum GitError {
    /// Git could not be started, read, signalled, or reaped.
    #[error("git in {cwd}: {source}")]
    Io {
        cwd: PathBuf,
        #[source]
        source: io::Error,
    },
    /// Git returned a failing status; its untrusted stderr is not displayed.
    #[error("git ls-files in {cwd} failed ({status})")]
    Failed { cwd: PathBuf, status: ExitStatus },
    /// Git or a descendant kept running or holding its pipes past the deadline.
    #[error("git ls-files in {cwd} exceeded its deadline")]
    Timeout { cwd: PathBuf },
    /// One output stream exceeded its byte limit.
    #[error("git ls-files in {cwd} exceeded the {stream} limit ({limit} bytes)")]
    OutputLimit {
        cwd: PathBuf,
        stream: &'static str,
        limit: usize,
    },
    /// Output was not a sequence of nonempty, NUL-terminated paths.
    #[error("git ls-files in {cwd} returned malformed NUL-delimited paths")]
    InvalidOutput { cwd: PathBuf },
    /// The platform has no implemented bounded process-group adapter.
    #[error("bounded git selection is not supported on this platform ({cwd})")]
    Unsupported { cwd: PathBuf },
}

/// List indexed `*.md` paths at and below `cwd`, relative to that directory.
///
/// Preserves Git's order and duplicates, including staged additions and unstaged
/// deletions. Untracked files are excluded. NUL delimiting preserves native path
/// bytes, including Unicode, quotes, tabs, and newlines on Unix.
///
/// The environment is inherited like the Python reference: `cwd` sets Git's
/// working directory, while variables such as `GIT_DIR` retain Git's own routing
/// semantics. This function does not change the caller's cwd or environment.
///
/// Git has a 10-second deadline, 16 MiB stdout limit, and 64 KiB stderr limit.
/// Failures never become an empty successful list. Linux and macOS have a process
/// adapter; only Linux has runtime test evidence. Other platforms return
/// `GitError::Unsupported` before spawning.
pub fn tracked_markdown(cwd: &Path) -> Result<Vec<PathBuf>, GitError> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        use std::process::Command;

        let mut command = Command::new("git");
        command.args(["ls-files", "-z", "--", "*.md"]);
        let output = unix::run(&mut command, cwd, unix::Limits::GIT)?;
        if output.is_empty() {
            return Ok(Vec::new());
        }
        let Some(paths) = output.strip_suffix(&[0]) else {
            return Err(GitError::InvalidOutput {
                cwd: cwd.to_owned(),
            });
        };
        paths
            .split(|byte| *byte == 0)
            .map(|path| {
                if path.is_empty() {
                    Err(GitError::InvalidOutput {
                        cwd: cwd.to_owned(),
                    })
                } else {
                    Ok(PathBuf::from(OsStr::from_bytes(path)))
                }
            })
            .collect()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    Err(GitError::Unsupported {
        cwd: cwd.to_owned(),
    })
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix {
    use super::*;
    use std::io::Read;
    use std::os::fd::AsFd;
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
    use rustix::io::Errno;
    use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, kill_process_group, waitid};

    pub(super) struct Limits {
        pub timeout: Duration,
        pub stdout: usize,
        pub stderr: usize,
    }

    impl Limits {
        pub const GIT: Self = Self {
            timeout: Duration::from_secs(10),
            stdout: 16 * 1024 * 1024,
            stderr: 64 * 1024,
        };
    }

    fn io_error(cwd: &Path, source: impl Into<io::Error>) -> GitError {
        GitError::Io {
            cwd: cwd.to_owned(),
            source: source.into(),
        }
    }

    pub(super) fn run(
        command: &mut Command,
        cwd: &Path,
        limits: Limits,
    ) -> Result<Vec<u8>, GitError> {
        command
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let mut child = command.spawn().map_err(|err| io_error(cwd, err))?;
        let result = capture(&mut child, cwd, &limits);
        // Both success and failure close all pipe handles before group cleanup.
        // WNOWAIT keeps the leader's PID reserved until the final wait.
        let cleanup = finish(&mut child).map_err(|err| io_error(cwd, err));
        let status = cleanup?;
        let output = result?;
        if !status.success() {
            return Err(GitError::Failed {
                cwd: cwd.to_owned(),
                status,
            });
        }
        Ok(output)
    }

    fn pid(child: &Child) -> io::Result<Pid> {
        i32::try_from(child.id())
            .ok()
            .filter(|id| *id > 1)
            .and_then(Pid::from_raw)
            .ok_or_else(|| io::Error::other("invalid Git process group ID"))
    }

    fn exited(pid: Pid) -> io::Result<bool> {
        match waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Ok(status) => Ok(status.is_some()),
            Err(Errno::INTR) => Ok(false),
            Err(err) => Err(err.into()),
        }
    }

    fn nonblocking(pipe: &impl AsFd) -> io::Result<()> {
        fcntl_setfl(pipe, fcntl_getfl(pipe)? | OFlags::NONBLOCK)?;
        Ok(())
    }

    fn capture(child: &mut Child, cwd: &Path, limits: &Limits) -> Result<Vec<u8>, GitError> {
        let pid = pid(child).map_err(|err| io_error(cwd, err))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| io_error(cwd, io::Error::other("missing Git stdout pipe")))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| io_error(cwd, io::Error::other("missing Git stderr pipe")))?;
        nonblocking(&stdout)
            .and_then(|()| nonblocking(&stderr))
            .map_err(|err| io_error(cwd, err))?;
        let start = Instant::now();
        let mut output = Vec::new();
        let mut stderr_len = 0;
        let (mut out_done, mut err_done) = (false, false);
        loop {
            if start.elapsed() >= limits.timeout {
                return Err(GitError::Timeout {
                    cwd: cwd.to_owned(),
                });
            }
            // One chunk per stream per iteration bounds memory and prevents a
            // continuously writing stream from starving the other or the timer.
            let mut buffer = [0; 8192];
            let out = read_chunk(&mut stdout, &mut buffer, &mut out_done)
                .map_err(|err| io_error(cwd, err))?;
            if out > limits.stdout.saturating_sub(output.len()) {
                return Err(GitError::OutputLimit {
                    cwd: cwd.to_owned(),
                    stream: "stdout",
                    limit: limits.stdout,
                });
            }
            output.extend_from_slice(&buffer[..out]);
            let err = read_chunk(&mut stderr, &mut buffer, &mut err_done)
                .map_err(|err| io_error(cwd, err))?;
            if err > limits.stderr.saturating_sub(stderr_len) {
                return Err(GitError::OutputLimit {
                    cwd: cwd.to_owned(),
                    stream: "stderr",
                    limit: limits.stderr,
                });
            }
            stderr_len += err;
            if out_done && err_done && exited(pid).map_err(|err| io_error(cwd, err))? {
                return Ok(output);
            }
            if out == 0 && err == 0 {
                thread::sleep(Duration::from_millis(2));
            }
        }
    }

    fn read_chunk(pipe: &mut impl Read, buffer: &mut [u8], done: &mut bool) -> io::Result<usize> {
        if *done {
            return Ok(0);
        }
        match pipe.read(buffer) {
            Ok(0) => {
                *done = true;
                Ok(0)
            }
            Ok(count) => Ok(count),
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(0)
            }
            Err(err) => Err(err),
        }
    }

    fn signal(pid: Pid, signal: Signal) -> io::Result<()> {
        match kill_process_group(pid, signal) {
            Ok(()) | Err(Errno::SRCH) => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    fn finish(child: &mut Child) -> io::Result<ExitStatus> {
        let cleanup = (|| {
            let pid = pid(child)?;
            let term = signal(pid, Signal::TERM);
            thread::sleep(Duration::from_millis(100));
            let kill = signal(pid, Signal::KILL);
            term.and(kill)
        })();
        // Attempt direct-child termination and reap even if group signalling failed.
        let fallback = if cleanup.is_err() {
            child.kill()
        } else {
            Ok(())
        };
        let waited = child.wait();
        cleanup?;
        fallback?;
        waited
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
#[path = "git_tests.rs"]
mod tests;
