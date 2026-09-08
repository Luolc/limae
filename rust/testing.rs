//! Shared budgets for the crate's own tests.

use std::time::Duration;

/// A test's correctness assertions must not depend on a wall-clock budget
/// unless the timeout is that test's subject. Tests whose subject is the
/// timeout pass a short budget at the call site; every other test uses this
/// one, which is wide enough never to fire. A shorter shared budget turns a
/// scheduling delay into a timeout error, and the correctness assertion then
/// fails on the error type.
pub(crate) const NEVER_ELAPSES: Duration = Duration::from_secs(600);
