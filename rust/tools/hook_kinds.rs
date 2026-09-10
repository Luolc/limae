//! List every `kind` the polish hook can write into its diagnostics.
//!
//! One name per line, in declaration order. `tools/check_repo_contracts.sh`
//! holds the `kind` table in `docs/knowledge/polish-hook-self-trial.md` to
//! this list: a category that exists in the code and not in the table is a
//! category nobody can act on, and the table has drifted once already.
//!
//! The list is exhaustive by construction, and there is one list. Each
//! [`every_variant!`] invocation is the only place its enum's variants are
//! named: the same tokens become the array `main` prints and the arms of a
//! `match` with no wildcard. A variant added to the enum and not to the
//! invocation is a missing arm, and that is a compile error; the only edit
//! that fixes it also puts the variant in the array, and the array is what
//! the gate compares with the handbook. There is no second place to update
//! and forget. Compiling is enough to trip the lock, and
//! `cargo clippy --all-targets` and `cargo test` both do.
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

/// Name every unit variant of an enum once, as an array and as a `match`.
///
/// `covered elsewhere` takes the patterns for variants that carry data and are
/// listed through another enum; a listed variant repeated is an unreachable
/// arm, which the build rejects as well.
macro_rules! every_variant {
    (
        $list:ident: $type:ident = [$($variant:ident),+ $(,)?]
        $(, covered elsewhere: [$($elsewhere:pat),+ $(,)?])?
    ) => {
        const $list: [$type; [$($type::$variant),+].len()] = [$($type::$variant),+];
        const _: () = {
            let mut index = 0;
            while index < $list.len() {
                match $list[index] {
                    $($type::$variant => {})+
                    $($($elsewhere => {})+)?
                }
                index += 1;
            }
        };
    };
}

every_variant!(REASONS: FailureReason = [
    NoEngine,
    NotInstalled,
    TimedOut,
    Unreachable,
    Rejected,
    NonzeroExit,
    EmptyAnswer,
    UnreadableAnswer,
    Other,
]);

every_variant!(
    OTHERS: Kind = [Incomplete, Repaired, Misconfigured, Crashed],
    covered elsewhere: [Kind::Engine(_)]
);

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
