use super::{
    DIAGNOSTICS_FILENAME, DIRECTORY_MODE, FILE_MODE, Kind, ORPHAN_RETENTION, PART_SUFFIX,
    PARTS_DIRECTORY, RETENTION, STATE_DIRECTORY, Step, TEMPORARY_SUFFIX, VOID_FILENAME, identifier,
    in_work_tree, instance, keep, note, part, prune, root, session, stale, timestamp, void, voided,
};

use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type TestResult = Result<(), Box<dyn Error>>;

/// A fixed clock reading, so that no assertion here depends on the wall clock.
const NOW: Duration = Duration::from_secs(1_700_000_000);

fn now() -> SystemTime {
    UNIX_EPOCH + NOW
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "limae-hook-state-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Set one directory's modification time, so that ageing is arithmetic on a
/// fixed clock and not a wait.
fn age(path: &Path, age: Duration) -> TestResult {
    let seconds = i64::try_from((NOW - age).as_secs())?;
    let stamp = rustix::fs::Timespec {
        tv_sec: seconds,
        tv_nsec: 0,
    };
    rustix::fs::utimensat(
        rustix::fs::CWD,
        path,
        &rustix::fs::Timestamps {
            last_access: stamp,
            last_modification: stamp,
        },
        rustix::fs::AtFlags::empty(),
    )?;
    Ok(())
}

fn mode(path: &Path) -> Result<u32, Box<dyn Error>> {
    Ok(fs::metadata(path)?.permissions().mode() & 0o777)
}

/// The names in one directory, sorted, so an assertion can be about all of them
/// rather than about the one it went looking for.
fn names(path: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let mut found = fs::read_dir(path)?
        .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    found.sort();
    Ok(found)
}

fn lines(path: &Path) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    fs::read_to_string(path)?
        .lines()
        .map(|line| Ok(serde_json::from_str(line)?))
        .collect()
}

fn environment(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    pairs
        .iter()
        .map(|(name, value)| (OsString::from(name), OsString::from(value)))
        .collect()
}

// -- ids becoming path segments ------------------------------------------

/// A UUID has to survive, or the encoding below is just renaming everything
/// and no arm here can tell the two apart.
#[test]
fn a_well_formed_id_is_left_exactly_as_it_arrived() {
    assert_eq!(
        identifier(Some("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")),
        "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
    );
}

/// Everything that is not a letter, a digit or a hyphen is spelled out as
/// bytes, the underscore included, so that no two ids share a name: the
/// pairs below would collide under a fold-to-underscore.
#[test]
fn an_id_that_names_a_parent_directory_is_encoded_into_one_segment() {
    assert_eq!(identifier(Some("../../escape")), "_2E_2E_2F_2E_2E_2Fescape");
    assert_eq!(identifier(Some("a/b")), "a_2Fb");
    assert_eq!(identifier(Some("a?b")), "a_3Fb");
    assert_eq!(identifier(Some("a_b")), "a_5Fb");
    assert_eq!(identifier(Some("..")), "_2E_2E");
    assert_eq!(identifier(Some("甲")), "_E7_94_B2");
    assert_ne!(identifier(Some("a/b")), identifier(Some("a?b")));
    assert_ne!(identifier(Some("a_2Fb")), identifier(Some("a/b")));
}

/// An id too long to be a file name is no id at all, rather than the first
/// part of one — cutting it short would make two ids one. The length is
/// spelled out rather than read back from `NAME_LIMIT`: an assertion against
/// the constant it is checking holds for every value of it.
#[test]
fn an_id_too_long_to_name_a_file_is_empty_and_so_is_a_missing_one() {
    assert_eq!(identifier(Some(&"x".repeat(120))), "x".repeat(120));
    assert_eq!(identifier(Some(&"x".repeat(121))), "");
    assert_eq!(identifier(Some(&"甲".repeat(41))), "");
    assert_eq!(identifier(None), "");
    assert_eq!(identifier(Some("")), "");
}

/// The point of the folding: a hostile session id creates a directory under the
/// root and nothing anywhere else.
#[test]
fn a_session_id_cannot_name_a_directory_outside_the_state_root() -> TestResult {
    // Two levels of directory this test owns, so that the `../../` a folding
    // failure would follow lands inside them and not in the machine's own
    // scratch directory.
    let scratch = TempDir::new("escape")?;
    let nested = scratch.path().join("nested");
    let root = nested.join(STATE_DIRECTORY);
    let directory = session(&root, &identifier(Some("../../escape")))?;

    assert_eq!(directory.parent(), Some(root.as_path()));
    assert_eq!(names(&root)?, vec!["_2E_2E_2F_2E_2E_2Fescape".to_owned()]);
    assert_eq!(names(&nested)?, vec![STATE_DIRECTORY.to_owned()]);
    assert_eq!(names(scratch.path())?, vec!["nested".to_owned()]);
    Ok(())
}

// -- where the state lives -----------------------------------------------

#[test]
fn the_state_root_is_scratch_and_a_fixed_name_under_it() -> TestResult {
    let scratch = TempDir::new("root")?;
    let env = environment(&[("TMPDIR", &scratch.path().to_string_lossy())]);
    assert_eq!(root(&env), Some(scratch.path().join(STATE_DIRECTORY)));
    Ok(())
}

/// No variable of this module's own moves it: a hook's environment is set by
/// whatever configured the session, so a name would not be a boundary.
#[test]
fn no_setting_but_the_system_scratch_directory_moves_the_state_root() -> TestResult {
    let scratch = TempDir::new("fixed")?;
    let elsewhere = TempDir::new("elsewhere")?;
    let env = environment(&[
        ("TMPDIR", &scratch.path().to_string_lossy()),
        ("LIMAE_HOOK_STATE", &elsewhere.path().to_string_lossy()),
        (
            "LIMAE_HOOK_STATE_FOR_TESTS",
            &elsewhere.path().to_string_lossy(),
        ),
    ]);
    assert_eq!(root(&env), Some(scratch.path().join(STATE_DIRECTORY)));
    Ok(())
}

/// A reply that lands in a working tree is one `git add` away from a public
/// repository (ADR-0009 五, 八).
#[test]
fn a_scratch_directory_inside_a_checkout_leaves_nowhere_to_put_a_reply() -> TestResult {
    let scratch = TempDir::new("checkout")?;
    let inside = scratch.path().join("repo");
    fs::create_dir(&inside)?;
    let env = environment(&[("TMPDIR", &inside.to_string_lossy())]);

    // The same directory, before and after it becomes a checkout: only the
    // `.git` differs, so the refusal cannot be coming from anything else.
    assert_eq!(root(&env), Some(inside.join(STATE_DIRECTORY)));
    fs::create_dir(inside.join(".git"))?;
    assert_eq!(root(&env), None);
    Ok(())
}

/// A worktree's `.git` is a file, not a directory, and it marks a checkout just
/// the same.
#[test]
fn a_worktree_counts_as_a_checkout_though_its_git_is_a_file() -> TestResult {
    let scratch = TempDir::new("worktree")?;
    let deep = scratch.path().join("a/b/c");
    fs::create_dir_all(&deep)?;
    assert!(!in_work_tree(&deep));
    fs::write(scratch.path().join("a/.git"), "gitdir: /elsewhere\n")?;
    assert!(in_work_tree(&deep));
    Ok(())
}

// -- permissions ---------------------------------------------------------

/// With the umask wide open, the mode can only come from the call that created
/// the file — a `chmod` afterwards would still leave a window in which the
/// reply was everyone's to read.
///
/// The umask is process-wide, so it is put back before this returns.
#[test]
fn nothing_holding_a_reply_is_created_readable_by_others() -> TestResult {
    let scratch = TempDir::new("modes")?;
    let root = scratch.path().join(STATE_DIRECTORY);
    let previous = rustix::process::umask(rustix::fs::Mode::empty());
    let checked = (|| -> TestResult {
        let directory = session(&root, "session")?;
        let parts = directory.join(PARTS_DIRECTORY).join("message");
        keep(&parts, 0, "一批。")?;
        void(&parts)?;
        note(&directory, "message", Step::Fix, Kind::Crashed, now());

        // Every level, not only the last one: a directory of directories of
        // replies is as much this user's own as the replies are.
        // The modes are written out rather than read back from
        // `DIRECTORY_MODE` and `FILE_MODE`: an assertion against the constant
        // it is checking holds for every value of it, `0o777` included.
        assert_eq!(mode(&root)?, 0o700);
        assert_eq!(mode(&directory)?, 0o700);
        assert_eq!(mode(&directory.join(PARTS_DIRECTORY))?, 0o700);
        assert_eq!(mode(&parts)?, 0o700);
        assert_eq!(mode(&part(&parts, 0))?, 0o600);
        assert_eq!(mode(&parts.join(VOID_FILENAME))?, 0o600);
        assert_eq!(mode(&directory.join(DIAGNOSTICS_FILENAME))?, 0o600);
        assert_eq!((DIRECTORY_MODE, FILE_MODE), (0o700, 0o600));
        Ok(())
    })();
    let _ = rustix::process::umask(previous);
    checked
}

/// A build that cannot set modes refuses before it creates anything.
///
/// What this arm proves and what it does not: the capability is passed in, so
/// the assertion runs here, on Unix, and goes red if the refusal is ever
/// ordered after the first `mkdir` — which is the whole of what the refusal is
/// for. It says nothing about how any particular non-Unix target behaves; no
/// such target is built or run in this repository, and this is not an
/// acceptance of one.
#[test]
fn a_build_without_file_modes_refuses_before_it_creates_anything() -> TestResult {
    let scratch = TempDir::new("nomodes")?;
    let directory = scratch.path().join(STATE_DIRECTORY).join("session");

    let refused = super::create_directory_with_modes(&directory, false)
        .err()
        .ok_or("a build without file modes must refuse")?;
    assert_eq!(refused.kind(), std::io::ErrorKind::Unsupported);
    // Not the leaf, and not the root above it either: nothing at all.
    assert_eq!(names(scratch.path())?, Vec::<String>::new());

    // The same call with the modes in hand creates it, so the refusal above is
    // not the answer this function gives to everything.
    super::create_directory_with_modes(&directory, true)?;
    assert!(directory.is_dir());
    // And this build is one that has them, or every other arm here is testing a
    // platform nobody is on. Asserted in a const block, so a `MODES` that read
    // the wrong platform would not compile rather than fail here.
    const { assert!(super::MODES) };
    Ok(())
}

// -- caching one batch ---------------------------------------------------

#[test]
fn a_batch_lands_under_its_index_and_leaves_nothing_half_written() -> TestResult {
    let scratch = TempDir::new("keep")?;
    let parts = scratch.path().join("parts/message");
    keep(&parts, 7, "第八批。")?;

    assert_eq!(names(&parts)?, vec![format!("000007{PART_SUFFIX}")]);
    assert_eq!(fs::read_to_string(part(&parts, 7))?, "第八批。");
    Ok(())
}

/// A batch already under an index is never replaced: the second `keep` is
/// refused, says so by kind, and the first batch is what stays. This is the
/// whole of what keeps two processes given the same index from silently
/// writing over each other.
#[test]
fn a_batch_already_under_an_index_is_never_written_over() -> TestResult {
    let scratch = TempDir::new("noreplace")?;
    let parts = scratch.path().join("parts/message");
    keep(&parts, 0, "甲")?;

    let refused = keep(&parts, 0, "乙")
        .err()
        .ok_or("a second batch under the index must be refused")?;

    assert_eq!(refused.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read_to_string(part(&parts, 0))?, "甲");
    assert_eq!(names(&parts)?, vec![format!("000000{PART_SUFFIX}")]);
    Ok(())
}

/// The temporary name carries this process's id and a counter, so a leftover
/// from a sibling that died mid-write does not stop the exclusive create.
#[test]
fn a_leftover_temporary_file_from_another_process_does_not_block_a_batch() -> TestResult {
    let scratch = TempDir::new("leftover")?;
    let parts = scratch.path().join("parts/message");
    fs::create_dir_all(&parts)?;
    fs::write(
        parts.join(format!("000000.4294967295.0{TEMPORARY_SUFFIX}")),
        "半",
    )?;

    keep(&parts, 0, "甲")?;
    assert_eq!(fs::read_to_string(part(&parts, 0))?, "甲");
    Ok(())
}

// -- one message instance ------------------------------------------------

/// The turn is part of the key, and the join is on the one character
/// [`identifier`] never lets through, so no two pairs of ids share a name.
#[test]
fn an_instance_is_named_by_its_message_and_its_turn() {
    assert_eq!(instance("m", "t"), "m.t");
    assert_ne!(instance("m", "t1"), instance("m", "t2"));
    assert_ne!(instance("a-b", "c"), instance("a", "b-c"));
    assert!(!identifier(Some("a.b")).contains('.'));
}

/// The mark is a file beside the batches, not the absence of the directory: a
/// sibling still waiting on this instance has to find what it was waiting for.
#[test]
fn a_voided_instance_keeps_its_batches_and_says_so_twice_without_complaint() -> TestResult {
    let scratch = TempDir::new("void")?;
    let parts = scratch.path().join("parts/message.turn");
    keep(&parts, 0, "甲\n")?;
    assert!(!voided(&parts));

    void(&parts)?;
    void(&parts)?;

    assert!(voided(&parts));
    assert_eq!(fs::read_to_string(part(&parts, 0))?, "甲\n");
    assert_eq!(
        names(&parts)?,
        vec!["000000.part".to_owned(), VOID_FILENAME.to_owned()]
    );

    // An instance nothing has been cached under yet can be abandoned too.
    let fresh = scratch.path().join("parts/fresh.turn");
    void(&fresh)?;
    assert!(voided(&fresh));
    Ok(())
}

// -- diagnostics ---------------------------------------------------------

#[test]
fn each_fail_open_path_appends_one_line_and_says_only_where_and_how() -> TestResult {
    let scratch = TempDir::new("note")?;
    let directory = scratch.path().join("session");
    note(&directory, "m1", Step::Siblings, Kind::Incomplete, now());
    note(&directory, "m2", Step::Fix, Kind::Misconfigured, now());

    let written = lines(&directory.join(DIAGNOSTICS_FILENAME))?;
    assert_eq!(written.len(), 2);
    for line in &written {
        let object = line.as_object().ok_or("not an object")?;
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["at", "kind", "message_id", "step"]);
    }
    assert_eq!(written[0]["step"], "siblings");
    assert_eq!(written[0]["kind"], "incomplete");
    assert_eq!(written[0]["message_id"], "m1");
    assert_eq!(written[1]["step"], "fix");
    assert_eq!(written[1]["kind"], "config");
    Ok(())
}

/// The names a reader looks a line up under, spelled out here because the
/// handbook's table and the diagnostics file have to agree.
#[test]
fn every_step_and_kind_is_written_down_under_the_name_it_is_documented_by() {
    assert_eq!(
        [Step::Siblings, Step::Fix, Step::Display].map(Step::as_str),
        ["siblings", "fix", "display"],
    );
    assert_eq!(
        [
            Kind::Incomplete,
            Kind::Partial,
            Kind::Unclosed,
            Kind::Misconfigured,
            Kind::Crashed,
        ]
        .map(Kind::as_str),
        ["incomplete", "partial", "unclosed", "config", "crashed"],
    );
}

/// A hook that cannot write its own diagnostics still has a reply to get out of
/// the way of.
#[test]
fn a_diagnostics_file_that_cannot_be_written_is_not_an_error() -> TestResult {
    let scratch = TempDir::new("unwritable")?;
    let blocked = scratch.path().join("file");
    fs::write(&blocked, "not a directory")?;
    note(
        &blocked.join("session"),
        "m1",
        Step::Display,
        Kind::Crashed,
        now(),
    );
    assert_eq!(fs::read_to_string(&blocked)?, "not a directory");
    Ok(())
}

#[test]
fn a_line_says_when_it_was_written_in_utc() {
    // 2023-11-14T22:13:20+00:00 is 1_700_000_000 seconds after the epoch.
    assert_eq!(timestamp(now()), "2023-11-14T22:13:20+00:00");
    assert_eq!(
        timestamp(now() + Duration::from_micros(1)),
        "2023-11-14T22:13:20.000001+00:00"
    );
    assert_eq!(timestamp(UNIX_EPOCH), "1970-01-01T00:00:00+00:00");
    assert_eq!(
        timestamp(UNIX_EPOCH + Duration::from_secs(1_709_164_800)),
        "2024-02-29T00:00:00+00:00"
    );
}

// -- the two horizons ----------------------------------------------------

/// The horizon that reaches a whole session, and the one that does not.
///
/// The middle arm is what keeps the two apart: an implementation whose session
/// retention was the orphan's would delete a session nobody has been in for two
/// hours, and one that deleted nothing would keep the day-old one.
#[test]
fn a_session_is_kept_for_a_day_and_not_for_an_orphan_s_hour() -> TestResult {
    let scratch = TempDir::new("sessions")?;
    let root = scratch.path().join(STATE_DIRECTORY);
    let fresh = session(&root, "fresh")?;
    let middling = session(&root, "middling")?;
    let old = session(&root, "old")?;
    // Fixed ages, not `RETENTION + …`: an arm stated in terms of the constant
    // it is checking moves with it and pins nothing.
    age(&middling, Duration::from_secs(2 * 3600))?;
    age(&old, Duration::from_secs(25 * 3600))?;

    prune(&root, now());

    assert!(fresh.is_dir());
    assert!(middling.is_dir());
    assert!(!old.exists());
    assert_eq!(RETENTION, Duration::from_secs(24 * 3600));
    Ok(())
}

/// Nothing but this sweep removes a message's batches: a finished message
/// leaves them (a sibling may still be reading), and a message the host is
/// killed in the middle of never gets a final batch at all. Its session is the
/// live one, so session retention does not reach it either.
///
/// The abandoned directory here is younger than [`RETENTION`], so an
/// implementation with only one horizon keeps it.
#[test]
fn the_batches_of_an_abandoned_message_go_before_their_session_does() -> TestResult {
    let scratch = TempDir::new("orphans")?;
    let root = scratch.path().join(STATE_DIRECTORY);
    let directory = session(&root, "live")?;
    let parts = directory.join(PARTS_DIRECTORY);
    let streaming = parts.join("still-streaming");
    let abandoned = parts.join("abandoned");
    for message in [&streaming, &abandoned] {
        keep(message, 0, "甲")?;
    }
    // Two hours: well past the orphans' horizon and nowhere near the
    // session's, which is the gap the two constants have to leave.
    age(&abandoned, Duration::from_secs(2 * 3600))?;

    prune(&root, now());

    assert!(!abandoned.exists());
    assert!(streaming.is_dir());
    assert!(directory.is_dir());
    assert_eq!(ORPHAN_RETENTION, Duration::from_secs(3600));
    Ok(())
}

#[test]
fn a_directory_that_cannot_be_aged_is_not_one_to_delete() -> TestResult {
    let scratch = TempDir::new("unaged")?;
    assert!(!stale(
        &scratch.path().join("missing"),
        now(),
        Duration::ZERO
    ));

    let present = scratch.path().join("present");
    fs::create_dir(&present)?;
    age(&present, Duration::from_secs(120))?;
    assert!(stale(&present, now(), Duration::from_secs(60)));
    assert!(!stale(&present, now(), Duration::from_secs(180)));
    Ok(())
}

/// A state root that is not there yet is not an error: the first run of a
/// session prunes before it has created anything.
#[test]
fn pruning_a_state_root_that_does_not_exist_does_nothing() -> TestResult {
    let scratch = TempDir::new("absent")?;
    prune(&scratch.path().join(STATE_DIRECTORY), now());
    assert_eq!(names(scratch.path())?, Vec::<String>::new());
    Ok(())
}
