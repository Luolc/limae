//! List every `kind` the polish hook can write into its diagnostics.
//!
//! One name per line, in declaration order. `tools/check_repo_contracts.sh`
//! holds the `kind` table in `docs/knowledge/polish-hook-self-trial.md` to
//! this list: a category that exists in the code and not in the table is a
//! category nobody can act on, and the table has drifted once already.
//!
//! The list is exhaustive by construction. `reason_position` and
//! `kind_position` are a `match` over each enum with no wildcard arm, so a new
//! variant stops this file compiling until it is named there, and the
//! constant assertions then stop it until it is in the list as well. That is
//! the property a text comparison alone cannot have: the gate reads the
//! handbook, but only this file can see the enum. Compiling it is enough to
//! trip the lock, and `cargo clippy --all-targets` and `cargo test` both do.
//!
//! The Cargo target is `[[example]]` for the same reason `render-lexicon` is:
//! it must not ship with `cargo install`. It reads nothing at run time, so it
//! needs no `include` entry and has nothing to say outside the repository
//! gate that runs it.
//!
//! ```sh
//! cargo run --example hook-kinds
//! ```

use std::io::{self, Write};
use std::process::ExitCode;

use limae::hook::state::Kind;
use limae::polish::diagnosis::FailureReason;

const REASONS: [FailureReason; 9] = [
    FailureReason::NoEngine,
    FailureReason::NotInstalled,
    FailureReason::TimedOut,
    FailureReason::Unreachable,
    FailureReason::Rejected,
    FailureReason::NonzeroExit,
    FailureReason::EmptyAnswer,
    FailureReason::UnreadableAnswer,
    FailureReason::Other,
];

const OTHERS: [Kind; 4] = [
    Kind::Incomplete,
    Kind::Repaired,
    Kind::Misconfigured,
    Kind::Crashed,
];

const fn reason_position(reason: FailureReason) -> usize {
    match reason {
        FailureReason::NoEngine => 0,
        FailureReason::NotInstalled => 1,
        FailureReason::TimedOut => 2,
        FailureReason::Unreachable => 3,
        FailureReason::Rejected => 4,
        FailureReason::NonzeroExit => 5,
        FailureReason::EmptyAnswer => 6,
        FailureReason::UnreadableAnswer => 7,
        FailureReason::Other => 8,
    }
}

const fn kind_position(kind: Kind) -> usize {
    match kind {
        Kind::Engine(_) => usize::MAX,
        Kind::Incomplete => 0,
        Kind::Repaired => 1,
        Kind::Misconfigured => 2,
        Kind::Crashed => 3,
    }
}

// Each list names every variant exactly once, at its own position.
const _: () = {
    let mut index = 0;
    while index < REASONS.len() {
        assert!(
            reason_position(REASONS[index]) == index,
            "REASONS is out of order"
        );
        index += 1;
    }
    let mut index = 0;
    while index < OTHERS.len() {
        assert!(
            kind_position(OTHERS[index]) == index,
            "OTHERS is out of order"
        );
        index += 1;
    }
};

fn main() -> ExitCode {
    let mut stdout = io::stdout().lock();
    let kinds = REASONS
        .iter()
        .map(|reason| Kind::Engine(*reason))
        .chain(OTHERS)
        .map(Kind::as_str);
    for kind in kinds {
        if writeln!(stdout, "{kind}").is_err() {
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
