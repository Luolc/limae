use super::GitError;
use super::unix::{Limits, run};
use crate::testing::NEVER_ELAPSES;
use std::error::Error;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn Error>>;

fn limits() -> Limits {
    Limits {
        // Not this test's subject; see `NEVER_ELAPSES`.
        timeout: NEVER_ELAPSES,
        stdout: 128 * 1024,
        stderr: 128 * 1024,
    }
}

#[test]
fn both_pipes_are_drained_and_limits_are_inclusive() -> TestResult {
    let mut command = Command::new("/bin/sh");
    command.args([
        "-c",
        "i=0; while [ $i -lt 20000 ]; do printf x; printf y >&2; i=$((i+1)); done",
    ]);
    let cwd = std::env::current_dir()?;
    let exact = Limits {
        stdout: 20000,
        stderr: 20000,
        ..limits()
    };
    assert_eq!(run(&mut command, &cwd, exact)?, vec![b'x'; 20000]);
    let out = run(
        &mut command,
        &cwd,
        Limits {
            stdout: 19999,
            ..limits()
        },
    );
    assert_matches!(
        out,
        Err(GitError::OutputLimit {
            stream: "stdout",
            limit: 19999,
            ..
        })
    );
    let err = run(
        &mut command,
        &cwd,
        Limits {
            stderr: 19999,
            ..limits()
        },
    );
    assert_matches!(
        err,
        Err(GitError::OutputLimit {
            stream: "stderr",
            limit: 19999,
            ..
        })
    );
    Ok(())
}

#[test]
fn timeout_failure_and_success_are_distinct() -> TestResult {
    let cwd = std::env::current_dir()?;
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "exec /bin/sleep 1"]);
    let start = Instant::now();
    assert_matches!(
        run(
            &mut command,
            &cwd,
            Limits {
                timeout: Duration::from_millis(30),
                ..limits()
            }
        ),
        Err(GitError::Timeout { .. })
    );
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(run(&mut command, &cwd, limits())?, Vec::<u8>::new());
    let mut failed = Command::new("/bin/sh");
    failed.args(["-c", "printf synthetic-error >&2; exit 7"]);
    assert_matches!(run(&mut failed, &cwd, limits()), Err(GitError::Failed { status, .. }) if status.code() == Some(7));
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn process_tree_recovery() -> TestResult {
    use rustix::io::Errno;
    use rustix::process::{Pid, Signal, WaitOptions, set_child_subreaper, waitpid};
    if std::env::var_os("LIMAE_TEST_GIT_REAPER").is_none() {
        // Isolate subreaper state from Cargo's parallel test process.
        let result = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "files::git::tests::process_tree_recovery",
                "--nocapture",
            ])
            .env("LIMAE_TEST_GIT_REAPER", "1")
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
    let root = std::env::temp_dir().join(format!("limae-git-reaper-{}", std::process::id()));
    fs::create_dir(&root)?;
    let result = (|| -> TestResult {
        // Only the two cases whose subject is the deadline carry a short one;
        // see `NEVER_ELAPSES`. The other two are decided by the stdout limit and
        // by `run` returning at all, and the promptness assertion below is what
        // detects a `run` that waits for the detached descendant.
        for (script, expected, timeout) in [
            (
                "trap '' TERM; /bin/sleep 3 & printf '%s' $! > grandchild; printf '%s' $$ > leader; wait",
                "timeout",
                Duration::from_millis(100),
            ),
            (
                "/bin/sleep 3 & printf '%s' $! > grandchild; printf '%s' $$ > leader; exit 0",
                "timeout",
                Duration::from_millis(100),
            ),
            (
                "/bin/sleep 3 >/dev/null 2>&1 & printf '%s' $! > grandchild; printf '%s' $$ > leader; exit 0",
                "success",
                NEVER_ELAPSES,
            ),
            (
                "trap '' TERM; /bin/sleep 3 & printf '%s' $! > grandchild; printf '%s' $$ > leader; while :; do printf xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx; done",
                "limit",
                NEVER_ELAPSES,
            ),
        ] {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", script]);
            let open_fds = fs::read_dir("/proc/self/fd")?.count();
            let start = Instant::now();
            let outcome = run(
                &mut command,
                &root,
                Limits {
                    timeout,
                    stdout: 1024,
                    ..limits()
                },
            );
            // The subject is how long `run` takes to return. Taking the reading
            // here keeps the reaping loop below, which is budgeted four
            // seconds, out of the window this assertion measures.
            let returned = start.elapsed();
            let leader = read_pid(&root.join("leader"))?;
            let descendant = read_pid(&root.join("grandchild"))?;
            assert_eq!(
                fs::read_dir("/proc/self/fd")?.count(),
                open_fds,
                "pipe descriptors leaked"
            );
            // Reap the adopted descendant even when the product assertion fails.
            let mut reaped = None;
            let until = Instant::now() + Duration::from_secs(4);
            while Instant::now() < until {
                reaped = waitpid(Some(descendant), WaitOptions::NOHANG)?;
                if reaped.is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let (_, status) = reaped.ok_or("descendant not reaped")?;
            assert!(
                returned < Duration::from_secs(2),
                "run returned in {returned:?}"
            );
            assert!(
                status.signaled(),
                "descendant completed normally: {status:?}"
            );
            if script.starts_with("trap") {
                assert_eq!(status.terminating_signal(), Some(Signal::KILL.as_raw()));
            }
            assert_matches!(
                waitpid(Some(leader), WaitOptions::NOHANG),
                Err(Errno::CHILD)
            );
            match expected {
                "timeout" => assert_matches!(outcome, Err(GitError::Timeout { .. })),
                "limit" => assert_matches!(outcome, Err(GitError::OutputLimit { .. })),
                _ => {
                    let _ = outcome?;
                }
            }
        }
        Ok(())
    })();
    let cleanup = fs::remove_dir_all(&root);
    result?;
    cleanup?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn read_pid(path: &Path) -> Result<rustix::process::Pid, Box<dyn Error>> {
    let raw = fs::read_to_string(path)?.parse::<i32>()?;
    rustix::process::Pid::from_raw(raw).ok_or_else(|| "invalid test pid".into())
}
