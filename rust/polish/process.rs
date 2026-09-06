//! Bounded synchronous execution for polish engines.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use thiserror::Error;

/// Everything supplied to one external process.
///
/// The environment is exact: the child inherits no variables other than the
/// entries in `env`. Callers decide whether to pass a preset allowlist or the
/// environment configured for a custom command.
pub struct ProcessRequest {
    /// Program followed by its arguments.
    pub argv: Vec<OsString>,
    /// Bytes written to the child's standard input before it is closed.
    pub stdin: Vec<u8>,
    /// Working directory for the child.
    pub cwd: PathBuf,
    /// Complete child environment.
    pub env: Vec<(OsString, OsString)>,
}

/// Time and output bounds for one invocation.
#[derive(Clone, Copy)]
pub struct RunLimits {
    /// Absolute deadline for child I/O and leader completion.
    ///
    /// Process-group cleanup starts afterward and can add `terminate_grace`
    /// plus the final direct-child wait.
    pub deadline: Instant,
    /// Delay after successful `SIGTERM` delivery and before `SIGKILL`.
    ///
    /// Every spawned invocation pays this full delay, whether capture succeeds
    /// or fails. The direct child remains unreaped until the final wait to
    /// reserve its process-group ID.
    pub terminate_grace: Duration,
    /// Maximum accepted standard output bytes, inclusive.
    pub stdout: usize,
    /// Maximum accepted standard error bytes, inclusive.
    pub stderr: usize,
}

/// Cooperative cancellation signal for a running invocation.
#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// Create an unset cancellation token.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Request cancellation. Repeated calls have the same effect as one call.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Return whether cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Captured business output from a process that exited normally or nonzero.
///
/// This type intentionally has no `Debug` or `Display` implementation: its
/// byte fields can contain prose or engine diagnostics and must not enter an
/// error or log accidentally.
pub struct ProcessOutput {
    /// Child exit status after it has been reaped.
    pub status: ExitStatus,
    /// Captured standard output.
    pub stdout: Vec<u8>,
    /// Captured standard error.
    pub stderr: Vec<u8>,
}

/// A child stream involved in a bounded I/O failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stream {
    /// Standard input.
    Stdin,
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

impl std::fmt::Display for Stream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Stdin => "stdin",
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        };
        formatter.write_str(name)
    }
}

/// Failure to complete and explicitly reap a bounded external process.
#[derive(Debug, Error)]
pub enum ProcessError {
    /// No executable was supplied.
    #[error("external process argv is empty")]
    EmptyArgv,
    /// The process could not be spawned.
    #[error("external process spawn failed: {source}")]
    Spawn {
        /// Operating-system failure without the request or child output.
        #[source]
        source: io::Error,
    },
    /// Bounded pipe I/O failed.
    #[error("external process {stream} I/O failed: {source}")]
    Io {
        /// Stream on which the failure occurred.
        stream: Stream,
        /// Operating-system failure without the request or child output.
        #[source]
        source: io::Error,
    },
    /// The absolute deadline arrived before all I/O and the leader completed.
    #[error("external process exceeded its deadline")]
    Timeout,
    /// The caller requested cancellation before completion.
    #[error("external process was cancelled")]
    Cancelled,
    /// One captured output stream exceeded its inclusive byte limit.
    #[error("external process exceeded the {stream} limit ({limit} bytes)")]
    OutputLimit {
        /// Stream that exceeded its limit.
        stream: Stream,
        /// Configured inclusive limit.
        limit: usize,
    },
    /// Process-group termination or direct-child wait failed.
    #[error("external process cleanup failed: {source}")]
    Cleanup {
        /// Operating-system cleanup failure without the request or child output.
        #[source]
        source: io::Error,
    },
    /// This platform has no bounded process-group adapter.
    #[error("bounded external processes are not supported on this platform")]
    Unsupported,
}

/// Run a process with bounded input, output, lifetime, and tree cleanup.
///
/// A nonzero exit is a completed invocation and is returned in
/// [`ProcessOutput`]. Timeout and cancellation are distinct errors. Every
/// spawned path closes the pipes, terminates the Unix process group, and waits
/// for the direct child before returning. Cleanup happens outside `deadline`;
/// if both capture and cleanup fail, [`ProcessError::Cleanup`] takes priority.
pub fn run(
    request: &ProcessRequest,
    limits: RunLimits,
    cancellation: &CancellationToken,
) -> Result<ProcessOutput, ProcessError> {
    #[cfg(unix)]
    {
        unix::run(request, limits, cancellation)
    }
    #[cfg(not(unix))]
    {
        let _ = (request, limits, cancellation);
        Err(ProcessError::Unsupported)
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::io::{Read, Write};
    use std::os::fd::AsFd;
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command, Stdio};
    use std::thread;

    use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
    use rustix::io::Errno;
    use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, kill_process_group, waitid};

    struct Captured {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    }

    pub(super) fn run(
        request: &ProcessRequest,
        limits: RunLimits,
        cancellation: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        let Some((program, args)) = request.argv.split_first() else {
            return Err(ProcessError::EmptyArgv);
        };
        if cancellation.is_cancelled() {
            return Err(ProcessError::Cancelled);
        }
        if Instant::now() >= limits.deadline {
            return Err(ProcessError::Timeout);
        }

        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(&request.cwd)
            .env_clear()
            .envs(request.env.iter().cloned())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let mut child = command
            .spawn()
            .map_err(|source| ProcessError::Spawn { source })?;

        let captured = capture(&mut child, &request.stdin, limits, cancellation);
        let status = finish(&mut child, limits.terminate_grace)
            .map_err(|source| ProcessError::Cleanup { source })?;
        let captured = captured?;
        Ok(ProcessOutput {
            status,
            stdout: captured.stdout,
            stderr: captured.stderr,
        })
    }

    fn pid(child: &Child) -> io::Result<Pid> {
        i32::try_from(child.id())
            .ok()
            .filter(|id| *id > 1)
            .and_then(Pid::from_raw)
            .ok_or_else(|| io::Error::other("invalid external process group ID"))
    }

    fn exited(pid: Pid) -> io::Result<bool> {
        match waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Ok(status) => Ok(status.is_some()),
            Err(Errno::INTR) => Ok(false),
            Err(source) => Err(source.into()),
        }
    }

    fn nonblocking(pipe: &impl AsFd, stream: Stream) -> Result<(), ProcessError> {
        fcntl_setfl(
            pipe,
            fcntl_getfl(pipe).map_err(|source| ProcessError::Io {
                stream,
                source: source.into(),
            })? | OFlags::NONBLOCK,
        )
        .map_err(|source| ProcessError::Io {
            stream,
            source: source.into(),
        })
    }

    fn capture(
        child: &mut Child,
        input: &[u8],
        limits: RunLimits,
        cancellation: &CancellationToken,
    ) -> Result<Captured, ProcessError> {
        let process = pid(child).map_err(|source| ProcessError::Cleanup { source })?;
        let stdin = child.stdin.take().ok_or_else(|| ProcessError::Io {
            stream: Stream::Stdin,
            source: io::Error::other("missing child stdin pipe"),
        })?;
        let mut stdout = child.stdout.take().ok_or_else(|| ProcessError::Io {
            stream: Stream::Stdout,
            source: io::Error::other("missing child stdout pipe"),
        })?;
        let mut stderr = child.stderr.take().ok_or_else(|| ProcessError::Io {
            stream: Stream::Stderr,
            source: io::Error::other("missing child stderr pipe"),
        })?;
        nonblocking(&stdin, Stream::Stdin)?;
        nonblocking(&stdout, Stream::Stdout)?;
        nonblocking(&stderr, Stream::Stderr)?;

        let mut stdin = Some(stdin);
        let mut input_offset = 0;
        let mut captured = Captured {
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        let (mut stdout_done, mut stderr_done) = (false, false);
        loop {
            if cancellation.is_cancelled() {
                return Err(ProcessError::Cancelled);
            }
            if Instant::now() >= limits.deadline {
                return Err(ProcessError::Timeout);
            }

            let wrote = write_chunk(&mut stdin, input, &mut input_offset)?;
            let read_stdout = read_chunk(
                &mut stdout,
                &mut captured.stdout,
                limits.stdout,
                Stream::Stdout,
                &mut stdout_done,
            )?;
            let read_stderr = read_chunk(
                &mut stderr,
                &mut captured.stderr,
                limits.stderr,
                Stream::Stderr,
                &mut stderr_done,
            )?;
            let leader_exited =
                exited(process).map_err(|source| ProcessError::Cleanup { source })?;
            if stdin.is_none() && stdout_done && stderr_done && leader_exited {
                return Ok(captured);
            }
            if wrote == 0 && read_stdout == 0 && read_stderr == 0 {
                thread::sleep(Duration::from_millis(2));
            }
        }
    }

    fn write_chunk(
        pipe: &mut Option<impl Write>,
        input: &[u8],
        offset: &mut usize,
    ) -> Result<usize, ProcessError> {
        if *offset == input.len() {
            *pipe = None;
            return Ok(0);
        }
        let Some(writer) = pipe.as_mut() else {
            return Ok(0);
        };
        let end = input.len().min(offset.saturating_add(8192));
        match writer.write(&input[*offset..end]) {
            Ok(0) => Err(ProcessError::Io {
                stream: Stream::Stdin,
                source: io::Error::from(io::ErrorKind::WriteZero),
            }),
            Ok(count) => {
                *offset += count;
                Ok(count)
            }
            Err(source)
                if matches!(
                    source.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(0)
            }
            Err(source) if source.kind() == io::ErrorKind::BrokenPipe => {
                *pipe = None;
                Ok(0)
            }
            Err(source) => Err(ProcessError::Io {
                stream: Stream::Stdin,
                source,
            }),
        }
    }

    fn read_chunk(
        pipe: &mut impl Read,
        output: &mut Vec<u8>,
        limit: usize,
        stream: Stream,
        done: &mut bool,
    ) -> Result<usize, ProcessError> {
        if *done {
            return Ok(0);
        }
        let mut buffer = [0; 8192];
        match pipe.read(&mut buffer) {
            Ok(0) => {
                *done = true;
                Ok(0)
            }
            Ok(count) if count > limit.saturating_sub(output.len()) => {
                Err(ProcessError::OutputLimit { stream, limit })
            }
            Ok(count) => {
                output.extend_from_slice(&buffer[..count]);
                Ok(count)
            }
            Err(source)
                if matches!(
                    source.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(0)
            }
            Err(source) => Err(ProcessError::Io { stream, source }),
        }
    }

    fn signal(process: Pid, signal: Signal) -> io::Result<bool> {
        match kill_process_group(process, signal) {
            Ok(()) => Ok(true),
            Err(Errno::SRCH) => Ok(false),
            Err(source) => Err(source.into()),
        }
    }

    fn finish(child: &mut Child, terminate_grace: Duration) -> io::Result<ExitStatus> {
        let cleanup: io::Result<()> = (|| {
            let process = pid(child)?;
            if signal(process, Signal::TERM)? {
                thread::sleep(terminate_grace);
                let _ = signal(process, Signal::KILL)?;
            }
            Ok(())
        })();
        // Reap the direct child even when process-group signalling fails.
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

#[cfg(all(test, unix))]
#[path = "process_tests.rs"]
mod tests;
