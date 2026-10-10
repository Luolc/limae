//! Shared budgets and helpers for the crate's own tests.

use std::path::Path;
use std::time::Duration;

/// A test's correctness assertions must not depend on a wall-clock budget
/// unless the timeout is that test's subject. Tests whose subject is the
/// timeout pass a short budget at the call site; every other test uses this
/// one, which is wide enough never to fire. A shorter shared budget turns a
/// scheduling delay into a timeout error, and the correctness assertion then
/// fails on the error type.
pub(crate) const NEVER_ELAPSES: Duration = Duration::from_secs(600);

/// Writes an executable `#!/bin/sh` script with `body` at `path`.
///
/// A `/bin/sh` child writes the file, never this process. The test binary
/// runs tests on many threads, and many of them spawn processes: a sibling
/// that forks while this process holds the script open for writing hands
/// the child a copy of that descriptor, and until the child execs, the
/// kernel refuses to exec the script (`ETXTBSY`, "Text file busy").
#[cfg(unix)]
pub(crate) fn write_script(path: &Path, body: &str) -> Result<(), std::io::Error> {
    let status = std::process::Command::new("/bin/sh")
        .args([
            "-c",
            r#"printf '#!/bin/sh\n%s\n' "$2" > "$1" && chmod 755 "$1""#,
            "sh",
        ])
        .arg(path)
        .arg(body)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "writing {} exited with {status}",
            path.display()
        )))
    }
}
