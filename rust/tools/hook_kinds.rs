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
//! invocation is a missing arm, and that is a compile error; the only edits
//! that fix it also put the variant's values in the array, and the array is
//! what the gate compares with the handbook. A variant that carries data is
//! named together with the constant array of its payloads, and every element
//! of that array is written into the output — there is no way to name a
//! variant for the `match` without also producing it. Compiling is enough
//! to trip the lock, and `cargo clippy --all-targets` and `cargo test` both do.
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

/// Name every variant of an enum once, as an array and as a `match`.
///
/// Unit variants are listed by name. A variant that carries data is written
/// `Variant(SOURCE)`, where `SOURCE` is a constant array of the payloads; the
/// array gets one element per payload, in order, ahead of the unit variants.
/// A variant named twice is an unreachable arm, which the build rejects too.
macro_rules! every_variant {
    (
        $list:ident: $type:ident = [$($unit:ident),+ $(,)?]
        $(, carrying: [$($carrier:ident($source:ident)),+ $(,)?])?
    ) => {
        const $list: [$type; [$($type::$unit),+].len() $($(+ $source.len())+)?] = {
            let units = [$($type::$unit),+];
            let mut out = [units[0]; [$($type::$unit),+].len() $($(+ $source.len())+)?];
            let mut next = 0;
            $($(
                let mut index = 0;
                while index < $source.len() {
                    out[next] = $type::$carrier($source[index]);
                    next += 1;
                    index += 1;
                }
            )+)?
            let mut index = 0;
            while index < units.len() {
                out[next] = units[index];
                next += 1;
                index += 1;
            }
            out
        };
        const _: () = {
            let mut index = 0;
            while index < $list.len() {
                match $list[index] {
                    $($type::$unit => {})+
                    $($($type::$carrier(_) => {})+)?
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
    KINDS: Kind = [Incomplete, Repaired, Misconfigured, Crashed],
    carrying: [Engine(REASONS)]
);

fn main() -> ExitCode {
    let mut stdout = io::stdout().lock();
    for kind in KINDS.map(Kind::as_str) {
        if writeln!(stdout, "{kind}").is_err() {
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
