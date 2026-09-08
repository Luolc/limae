use super::{CancellationToken, ProcessError, ProcessRequest, RunLimits, Stream, run};
use std::error::Error;
use std::ffi::OsString;
#[cfg(target_os = "linux")]
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn Error>>;

fn request(cwd: &Path, script: &str) -> ProcessRequest {
    ProcessRequest {
        argv: vec!["/bin/sh".into(), "-c".into(), script.into()],
        stdin: Vec::new(),
        cwd: cwd.to_owned(),
        env: Vec::new(),
    }
}

fn limits() -> RunLimits {
    RunLimits {
        deadline: Instant::now() + Duration::from_secs(2),
        terminate_grace: Duration::from_millis(50),
        stdout: 128 * 1024,
        stderr: 128 * 1024,
    }
}

#[test]
fn empty_argv_is_rejected_before_spawning() {
    let command = ProcessRequest {
        argv: Vec::new(),
        stdin: Vec::new(),
        cwd: PathBuf::new(),
        env: Vec::new(),
    };
    assert!(matches!(
        run(&command, limits(), &CancellationToken::new()),
        Err(ProcessError::EmptyArgv)
    ));
}

#[test]
fn argv_stdin_cwd_and_exact_environment_reach_the_child() -> TestResult {
    let cwd = std::env::current_dir()?;
    let mut command = request(
        &cwd,
        "IFS= read -r line; printf '%s|%s' \"$line\" \"$ACME_VALUE\"; pwd >&2",
    );
    command.stdin = b"polish me\n".to_vec();
    command.env = vec![(OsString::from("ACME_VALUE"), OsString::from("synthetic"))];
    let output = run(&command, limits(), &CancellationToken::new())?;
    assert!(output.status.success());
    assert_eq!(output.stdout, b"polish me|synthetic");
    assert_eq!(output.stderr, format!("{}\n", cwd.display()).as_bytes());
    Ok(())
}

#[test]
fn nonzero_exit_is_completed_output_not_an_execution_error() -> TestResult {
    let cwd = std::env::current_dir()?;
    let output = run(
        &request(&cwd, "printf answer; printf diagnostic >&2; exit 7"),
        limits(),
        &CancellationToken::new(),
    )?;
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, b"answer");
    assert_eq!(output.stderr, b"diagnostic");
    Ok(())
}

#[test]
fn execution_errors_do_not_carry_request_or_child_bytes() -> TestResult {
    let cwd = std::env::current_dir()?;
    let sensitive = "synthetic-sensitive-value";
    let mut command = request(&cwd, &format!("printf {sensitive}"));
    command.stdin = sensitive.as_bytes().to_vec();
    command.env = vec![(sensitive.into(), sensitive.into())];
    let error = run(
        &command,
        RunLimits {
            stdout: 0,
            ..limits()
        },
        &CancellationToken::new(),
    )
    .err()
    .ok_or("output limit unexpectedly succeeded")?;
    assert!(!error.to_string().contains(sensitive));
    assert!(!format!("{error:?}").contains(sensitive));

    let missing = ProcessRequest {
        argv: vec![sensitive.into()],
        stdin: Vec::new(),
        cwd,
        env: Vec::new(),
    };
    let error = run(&missing, limits(), &CancellationToken::new())
        .err()
        .ok_or("missing executable unexpectedly ran")?;
    assert!(!error.to_string().contains(sensitive));
    assert!(!format!("{error:?}").contains(sensitive));
    if let Some(source) = error.source() {
        assert!(!source.to_string().contains(sensitive));
    }
    Ok(())
}

#[test]
fn both_pipes_are_drained_and_byte_limits_are_inclusive() -> TestResult {
    let cwd = std::env::current_dir()?;
    let command = request(
        &cwd,
        "i=0; while [ $i -lt 20000 ]; do printf x; printf y >&2; i=$((i+1)); done",
    );
    let exact = RunLimits {
        stdout: 20_000,
        stderr: 20_000,
        ..limits()
    };
    let output = run(&command, exact, &CancellationToken::new())?;
    assert_eq!(output.stdout, vec![b'x'; 20_000]);
    assert_eq!(output.stderr, vec![b'y'; 20_000]);

    let stdout = run(
        &command,
        RunLimits {
            stdout: 19_999,
            ..limits()
        },
        &CancellationToken::new(),
    );
    assert!(matches!(
        stdout,
        Err(ProcessError::OutputLimit {
            stream: Stream::Stdout,
            limit: 19_999
        })
    ));
    let stderr = run(
        &command,
        RunLimits {
            stderr: 19_999,
            ..limits()
        },
        &CancellationToken::new(),
    );
    assert!(matches!(
        stderr,
        Err(ProcessError::OutputLimit {
            stream: Stream::Stderr,
            limit: 19_999
        })
    ));
    Ok(())
}

#[test]
fn blocked_stdin_cannot_escape_the_deadline() -> TestResult {
    let cwd = std::env::current_dir()?;
    let mut command = request(&cwd, "exec /bin/sleep 2");
    command.stdin = vec![b'x'; 1024 * 1024];
    let started = Instant::now();
    assert!(matches!(
        run(
            &command,
            RunLimits {
                deadline: Instant::now() + Duration::from_millis(40),
                ..limits()
            },
            &CancellationToken::new()
        ),
        Err(ProcessError::Timeout)
    ));
    assert!(started.elapsed() < Duration::from_secs(1));

    let output = run(
        &request(&cwd, "printf done"),
        limits(),
        &CancellationToken::new(),
    )?;
    assert_eq!(output.stdout, b"done");
    Ok(())
}

#[test]
fn cancellation_is_distinct_from_timeout_and_an_unset_token_succeeds() -> TestResult {
    let cwd = std::env::current_dir()?;
    let token = CancellationToken::new();
    let trigger = token.clone();
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        trigger.cancel();
    });
    let started = Instant::now();
    let result = run(&request(&cwd, "exec /bin/sleep 2"), limits(), &token);
    canceller.join().map_err(|_| "canceller panicked")?;
    assert!(matches!(result, Err(ProcessError::Cancelled)));
    assert!(started.elapsed() < Duration::from_secs(1));

    let output = run(
        &request(&cwd, "printf active"),
        limits(),
        &CancellationToken::new(),
    )?;
    assert_eq!(output.stdout, b"active");
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn process_tree_is_terminated_and_reaped_on_every_completion_path() -> TestResult {
    use rustix::io::Errno;
    use rustix::process::{Pid, Signal, WaitOptions, set_child_subreaper, waitpid};

    if std::env::var_os("LIMAE_TEST_PROCESS_REAPER").is_none() {
        // Keep Linux subreaper state out of Cargo's parallel test process.
        let result = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "polish::process::tests::process_tree_is_terminated_and_reaped_on_every_completion_path",
                "--nocapture",
            ])
            .env("LIMAE_TEST_PROCESS_REAPER", "1")
            .output()?;
        assert!(
            result.status.success(),
            "isolated re-exec failed: {}\n--- child stdout ---\n{}\n--- child stderr ---\n{}",
            result.status,
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return Ok(());
    }

    set_child_subreaper(Pid::from_raw(1))?;
    let root = std::env::temp_dir().join(format!("limae-process-reaper-{}", std::process::id()));
    fs::create_dir(&root)?;
    let result = (|| -> TestResult {
        for case in cases() {
            let mut command = request(&root, case.script);
            command.stdin = case.stdin;
            let token = CancellationToken::new();
            let canceller = case.cancel_after.map(|delay| {
                let trigger = token.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(delay);
                    trigger.cancel();
                })
            });
            let open_fds = fs::read_dir("/proc/self/fd")?.count();
            let started = Instant::now();
            let outcome = run(
                &command,
                RunLimits {
                    deadline: Instant::now() + Duration::from_millis(100),
                    terminate_grace: Duration::from_millis(50),
                    stdout: case.stdout_limit,
                    stderr: 1024,
                },
                &token,
            );
            if let Some(handle) = canceller {
                handle.join().map_err(|_| "canceller panicked")?;
            }
            let leader = read_pid(&root.join("leader"))?;
            let descendant = read_pid(&root.join("descendant"))?;
            assert_eq!(
                fs::read_dir("/proc/self/fd")?.count(),
                open_fds,
                "pipe descriptors leaked in {}",
                case.name
            );

            let mut reaped = None;
            let reap_deadline = Instant::now() + Duration::from_secs(4);
            while Instant::now() < reap_deadline {
                reaped = waitpid(Some(descendant), WaitOptions::NOHANG)?;
                if reaped.is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let (_, status) = reaped.ok_or("descendant not reaped")?;
            assert!(started.elapsed() < Duration::from_secs(1), "{}", case.name);
            assert!(status.signaled(), "{} completed normally", case.name);
            if case.expect_kill {
                assert_eq!(
                    status.terminating_signal(),
                    Some(Signal::KILL.as_raw()),
                    "{}",
                    case.name
                );
            }
            assert!(matches!(
                waitpid(Some(leader), WaitOptions::NOHANG),
                Err(Errno::CHILD)
            ));
            (case.assert_outcome)(outcome);
            fs::remove_file(root.join("leader"))?;
            fs::remove_file(root.join("descendant"))?;
        }
        Ok(())
    })();
    let cleanup = fs::remove_dir_all(&root);
    result?;
    cleanup?;
    Ok(())
}

#[cfg(target_os = "linux")]
struct TreeCase {
    name: &'static str,
    script: &'static str,
    stdin: Vec<u8>,
    cancel_after: Option<Duration>,
    stdout_limit: usize,
    expect_kill: bool,
    assert_outcome: fn(Result<super::ProcessOutput, ProcessError>),
}

#[cfg(target_os = "linux")]
fn cases() -> [TreeCase; 4] {
    [
        TreeCase {
            name: "leader exited while descendant held pipes",
            script: "/bin/sleep 3 & printf '%s' $! > descendant; printf '%s' $$ > leader; exit 0",
            stdin: Vec::new(),
            cancel_after: None,
            stdout_limit: 1024,
            expect_kill: false,
            assert_outcome: |outcome| assert!(matches!(outcome, Err(ProcessError::Timeout))),
        },
        TreeCase {
            name: "successful leader left a detached-output descendant",
            script: "/bin/sleep 3 >/dev/null 2>&1 & printf '%s' $! > descendant; printf '%s' $$ > leader; exit 0",
            stdin: Vec::new(),
            cancel_after: None,
            stdout_limit: 1024,
            expect_kill: false,
            assert_outcome: |outcome| {
                assert!(outcome.is_ok());
            },
        },
        TreeCase {
            name: "cancellation escalated after ignored TERM",
            script: "trap '' TERM; /bin/sleep 3 & printf '%s' $! > descendant; printf '%s' $$ > leader; wait",
            stdin: Vec::new(),
            cancel_after: Some(Duration::from_millis(30)),
            stdout_limit: 1024,
            expect_kill: true,
            assert_outcome: |outcome| assert!(matches!(outcome, Err(ProcessError::Cancelled))),
        },
        TreeCase {
            name: "output limit terminated a live tree",
            script: "trap '' TERM; /bin/sleep 3 & printf '%s' $! > descendant; printf '%s' $$ > leader; while :; do printf xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx; done",
            stdin: Vec::new(),
            cancel_after: None,
            stdout_limit: 1024,
            expect_kill: true,
            assert_outcome: |outcome| {
                assert!(matches!(
                    outcome,
                    Err(ProcessError::OutputLimit {
                        stream: Stream::Stdout,
                        limit: 1024
                    })
                ));
            },
        },
    ]
}

#[cfg(target_os = "linux")]
fn read_pid(path: &PathBuf) -> Result<rustix::process::Pid, Box<dyn Error>> {
    let raw = fs::read_to_string(path)?.parse::<i32>()?;
    rustix::process::Pid::from_raw(raw).ok_or_else(|| "invalid test pid".into())
}
