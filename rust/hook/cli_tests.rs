use super::{BAD_USAGE, MESSAGE_DISPLAY, OK, STOP, is_codex_stop, message, run};
use crate::hook::ab::PENDING_FILENAME;
use crate::hook::render::BLOCK_GAP;
use crate::hook::state::{DIAGNOSTICS_FILENAME, PARTS_DIRECTORY, RETENTION, STATE_DIRECTORY, part};

use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, FileTimes, Permissions};
use std::io::Cursor;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value, json};

use crate::polish::diagnosis::FailureReason;

type TestResult = Result<(), Box<dyn Error>>;

/// A fixed clock reading, so that no assertion here depends on the wall clock.
fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

/// The ids the host sends, in the shape it sends them: UUIDs, which survive
/// sanitising unchanged.
const SESSION: &str = "11111111-2222-3333-4444-555555555555";
const MESSAGE: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

/// One sentence of synthetic Chinese prose that this repository's own rules
/// leave alone, so that a rewrite made of it changes only where a test changes
/// it.
const LINE: &str = "ACME 的报告写得不好，请把它改得像人话一些。";

/// What the engine stubs answer with: the same sentence, tidied, so a block is
/// built rather than the "no change" one.
const REWRITE: &str = "ACME 的报告写得不好 —— 请把它改得像人话一些。";

/// The marker every ordinary block carries on screen, spelled as
/// [`crate::hook::block`] spells it so that the newlines before it can be
/// counted.
const HEADING: &str = "── 润色 ──";

/// A message long enough to be worth polishing.
fn long() -> String {
    LINE.repeat(20)
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "limae-hook-cli-{name}-{}-{}",
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

/// What one run of the subcommand did.
struct Ran {
    code: u8,
    stdout: String,
    stderr: String,
}

impl Ran {
    /// The JSON object this run printed, if it printed one.
    fn answer(&self) -> Result<Option<Value>, Box<dyn Error>> {
        if self.stdout.is_empty() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&self.stdout)?))
    }

    /// What the host would put on screen in place of this batch, empty when the
    /// hook said nothing and the host paints the batch itself.
    fn displayed(&self) -> Result<String, Box<dyn Error>> {
        Ok(self
            .answer()?
            .as_ref()
            .and_then(|answer| answer.pointer("/hookSpecificOutput/displayContent"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned())
    }
}

/// One run's five directories: a `PATH` of fake CLIs, a home nothing real lives
/// in, the scratch directory the state root is derived from, the directory the
/// payload names, and the process's own working directory.
///
/// The last two are deliberately different, so that a test which configures an
/// engine in `cwd` is also saying the payload is what decides where the
/// configuration is read from.
///
/// No assertion here depends on what this machine has installed or on what the
/// user running the suite has configured.
struct Fixture {
    bin: TempDir,
    home: TempDir,
    scratch: TempDir,
    cwd: TempDir,
    elsewhere: TempDir,
}

impl Fixture {
    fn new(name: &str) -> Result<Self, std::io::Error> {
        Ok(Self {
            bin: TempDir::new(&format!("{name}-bin"))?,
            home: TempDir::new(&format!("{name}-home"))?,
            scratch: TempDir::new(&format!("{name}-scratch"))?,
            cwd: TempDir::new(&format!("{name}-cwd"))?,
            elsewhere: TempDir::new(&format!("{name}-elsewhere"))?,
        })
    }

    /// The environment of a run, plus whatever knobs a test sets.
    ///
    /// Sampling is off unless a test asks for it: what these tests control is
    /// the host protocol, never chance.
    fn with(&self, extra: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        let mut env: Vec<(OsString, OsString)> = [
            ("PATH", self.bin.path().as_os_str().to_owned()),
            ("HOME", self.home.path().as_os_str().to_owned()),
            (
                "XDG_CACHE_HOME",
                self.home.path().join("cache").into_os_string(),
            ),
            // Where scratch goes is where the state goes: the hook has no
            // setting of its own for it, on purpose.
            ("TMPDIR", self.scratch.path().as_os_str().to_owned()),
            ("LIMAE_HOOK_AB_RATE", OsString::from("0")),
        ]
        .into_iter()
        .map(|(name, value)| (OsString::from(name), value))
        .collect();
        env.extend(
            extra
                .iter()
                .map(|(name, value)| (OsString::from(*name), OsString::from(*value))),
        );
        env
    }

    /// Put one executable of the given name on the fake `PATH`.
    ///
    /// The engine's own `PATH` is the fake one, which holds these scripts and
    /// nothing else, so a script that needs `cat` or `grep` gets this run's real
    /// `PATH` back at the top of itself. Every stub also records that it ran, so
    /// that a test can say how many engine calls a turn made.
    fn install(&self, name: &str, script: &str) -> TestResult {
        let tools = std::env::var("PATH").unwrap_or_default();
        let path = self.bin.path().join(name);
        fs::write(
            &path,
            format!(
                "#!/bin/sh\nPATH='{tools}'\nexport PATH\necho run >> '{}'\n{script}\n",
                self.log().display()
            ),
        )?;
        fs::set_permissions(&path, Permissions::from_mode(0o755))?;
        Ok(())
    }

    /// Install the engine every turn of these tests runs, answering `REWRITE`.
    fn engine(&self) -> TestResult {
        self.install(
            "claude",
            &format!("cat > /dev/null\ncat <<'ANSWER'\n{REWRITE}\nANSWER"),
        )?;
        self.configure("[polish]\nengine = \"claude\"\n")
    }

    /// Install an engine that keeps what it was asked to rewrite.
    fn recording(&self) -> TestResult {
        self.install(
            "claude",
            &format!(
                "cat > '{}'\ncat <<'ANSWER'\n{REWRITE}\nANSWER",
                self.asked().display()
            ),
        )?;
        self.configure("[polish]\nengine = \"claude\"\n")
    }

    fn asked(&self) -> PathBuf {
        self.home.path().join("asked")
    }

    fn log(&self) -> PathBuf {
        self.home.path().join("calls")
    }

    /// How many times any stub engine has run.
    fn calls(&self) -> usize {
        fs::read_to_string(self.log()).map_or(0, |log| log.lines().count())
    }

    /// Write the `[polish]` table this run's configuration holds, where the
    /// payload says to look for it.
    fn configure(&self, table: &str) -> TestResult {
        fs::write(self.cwd.path().join("limae.toml"), table)?;
        Ok(())
    }

    /// Write the same table where the *process* is running instead, which is
    /// the fallback for a payload that names no directory.
    fn configure_process(&self, table: &str) -> TestResult {
        fs::write(self.elsewhere.path().join("limae.toml"), table)?;
        Ok(())
    }

    fn root(&self) -> PathBuf {
        self.scratch.path().join(STATE_DIRECTORY)
    }

    fn session(&self) -> PathBuf {
        self.root().join(SESSION)
    }

    fn parts(&self, message: &str) -> PathBuf {
        self.session().join(PARTS_DIRECTORY).join(message)
    }

    /// The `(step, kind)` pairs of this session's diagnostics, in order.
    fn steps(&self) -> Result<Vec<(String, String)>, Box<dyn Error>> {
        self.lines()?
            .iter()
            .map(|line| {
                Ok((
                    field(line, "step").to_owned(),
                    field(line, "kind").to_owned(),
                ))
            })
            .collect()
    }

    /// The fail-open lines this session has written.
    fn lines(&self) -> Result<Vec<Value>, Box<dyn Error>> {
        let path = self.session().join(DIAGNOSTICS_FILENAME);
        if !path.is_file() {
            return Ok(Vec::new());
        }
        fs::read_to_string(path)?
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| Ok(serde_json::from_str(line)?))
            .collect()
    }

    /// Run one hook event, with the payload as the process would receive it.
    fn hook(&self, payload: &Value, extra: &[(&str, &str)]) -> Result<Ran, Box<dyn Error>> {
        self.raw(&serde_json::to_string(payload)?, extra)
    }

    /// Run one hook event over exactly these bytes of stdin.
    fn raw(&self, stdin: &str, extra: &[(&str, &str)]) -> Result<Ran, Box<dyn Error>> {
        self.argv(&[], stdin, extra)
    }

    /// Run the subcommand with arguments after `hook`, which no host sends.
    fn argv(
        &self,
        args: &[&str],
        stdin: &str,
        extra: &[(&str, &str)],
    ) -> Result<Ran, Box<dyn Error>> {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let mut input = Cursor::new(stdin.as_bytes().to_vec());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            &args,
            self.elsewhere.path(),
            &self.with(extra),
            &mut input,
            &mut stdout,
            &mut stderr,
            now(),
        );
        Ok(Ran {
            code,
            stdout: String::from_utf8(stdout)?,
            stderr: String::from_utf8(stderr)?,
        })
    }
}

/// Backdate one directory, so that the sweep has something older than its
/// horizon to find at the fixed clock reading these tests run at.
fn aged(path: &Path, age: Duration) -> TestResult {
    let when = now() - age;
    fs::File::open(path)?.set_times(FileTimes::new().set_modified(when))?;
    Ok(())
}

fn field<'a>(line: &'a Value, key: &str) -> &'a str {
    line.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// One `MessageDisplay` batch, as Claude Code sends it.
fn batch(fixture: &Fixture, delta: &str, index: u64, final_batch: bool, message: &str) -> Value {
    json!({
        "session_id": SESSION,
        "transcript_path": "/dev/null",
        "cwd": fixture.cwd.path().to_string_lossy(),
        "hook_event_name": MESSAGE_DISPLAY,
        "turn_id": "turn",
        "message_id": message,
        "index": index,
        "final": final_batch,
        "delta": delta,
    })
}

/// One whole message in one batch, which is the shortest way to a block.
fn whole(fixture: &Fixture, text: &str) -> Value {
    batch(fixture, text, 0, true, MESSAGE)
}

/// Claude Code's `Stop`: no `model`, and the reply carried for the transcript's
/// sake rather than for polishing.
fn claude_stop() -> Value {
    json!({
        "session_id": SESSION,
        "transcript_path": "/dev/null",
        "cwd": "/nonexistent",
        "hook_event_name": STOP,
        "stop_hook_active": false,
        "last_assistant_message": LINE,
    })
}

/// Codex's `Stop`: the whole reply, and a `model` to say whose event this is.
fn codex(fixture: &Fixture, text: &str, ids: &[(&str, &str)]) -> Value {
    let mut payload = json!({
        "session_id": SESSION,
        "transcript_path": "/dev/null",
        "cwd": fixture.cwd.path().to_string_lossy(),
        "hook_event_name": STOP,
        "model": "synthetic-model",
        "stop_hook_active": false,
        "last_assistant_message": text,
    });
    for (key, value) in ids {
        payload[*key] = Value::String((*value).to_owned());
    }
    payload
}

fn object(payload: &Value) -> Result<Map<String, Value>, Box<dyn Error>> {
    Ok(payload
        .as_object()
        .ok_or("the payload builders make objects")?
        .clone())
}

/// Leave the pending note a sampled turn leaves, without running a trial.
fn pending(fixture: &Fixture, code: &str) -> TestResult {
    let session = fixture.session();
    fs::create_dir_all(&session)?;
    fs::write(
        session.join(PENDING_FILENAME),
        json!({ "code": code }).to_string(),
    )?;
    Ok(())
}

/// Count the newlines standing between what is already on screen and the block.
fn gap(screen: &str) -> Result<usize, Box<dyn Error>> {
    let (head, _) = screen
        .split_once(HEADING)
        .ok_or_else(|| format!("no block on screen: {screen:?}"))?;
    Ok(head.len() - head.trim_end_matches('\n').len())
}

// -- the three shapes on the wire -----------------------------------------

/// A `MessageDisplay` answer replaces the batch, and does it through the field
/// that host reads. Asserting only that some JSON was printed cannot tell this
/// event's shape from either `Stop`'s.
#[test]
fn a_display_batch_answers_through_display_content() -> TestResult {
    let fixture = Fixture::new("shape-display")?;
    fixture.engine()?;

    let ran = fixture.hook(&whole(&fixture, &long()), &[])?;
    let answer = ran.answer()?.ok_or("the final batch is answered")?;

    assert_eq!(ran.code, OK);
    assert_eq!(
        answer
            .pointer("/hookSpecificOutput/hookEventName")
            .and_then(Value::as_str),
        Some(MESSAGE_DISPLAY)
    );
    assert!(ran.displayed()?.contains(HEADING), "{answer}");
    assert!(answer.get("systemMessage").is_none(), "{answer}");
    assert!(
        answer
            .pointer("/hookSpecificOutput/additionalContext")
            .is_none(),
        "{answer}"
    );
    Ok(())
}

/// Codex has no display-replacement event, so its rewrite arrives below the
/// original as a `systemMessage` and never as a `displayContent` that would
/// claim to replace it (ADR-0014).
#[test]
fn a_codex_stop_answers_through_a_system_message() -> TestResult {
    let fixture = Fixture::new("shape-codex")?;
    fixture.engine()?;

    let ran = fixture.hook(&codex(&fixture, &long(), &[("turn_id", "turn")]), &[])?;
    let answer = ran.answer()?.ok_or("a Codex reply is answered")?;

    assert_eq!(ran.code, OK);
    let warning = answer
        .get("systemMessage")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(warning.contains(HEADING), "{answer}");
    // The block is written for a screen it ends by scrolling off; this one is
    // a paragraph inside a host's own message, so it stops where it stops.
    assert_eq!(warning.trim_end(), warning, "{answer}");
    assert!(answer.get("hookSpecificOutput").is_none(), "{answer}");
    Ok(())
}

/// Claude Code's `Stop` carries the A/B code name to the model, through the
/// third field and not either of the other two.
#[test]
fn a_claude_stop_answers_through_additional_context() -> TestResult {
    let fixture = Fixture::new("shape-stop")?;
    pending(&fixture, "灯塔")?;

    let ran = fixture.hook(&claude_stop(), &[])?;
    let answer = ran.answer()?.ok_or("a turn with a trial is answered")?;

    assert_eq!(ran.code, OK);
    assert_eq!(
        answer
            .pointer("/hookSpecificOutput/hookEventName")
            .and_then(Value::as_str),
        Some(STOP)
    );
    let context = answer
        .pointer("/hookSpecificOutput/additionalContext")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(context.contains("灯塔"), "{answer}");
    assert!(answer.get("systemMessage").is_none(), "{answer}");
    assert!(
        answer
            .pointer("/hookSpecificOutput/displayContent")
            .is_none(),
        "{answer}"
    );
    Ok(())
}

/// An event this hook does not handle leaves the host alone entirely: no JSON
/// at all, rather than an empty object the host would have to interpret.
#[test]
fn an_event_this_hook_does_not_handle_prints_nothing_at_all() -> TestResult {
    let fixture = Fixture::new("shape-other")?;
    fixture.engine()?;
    let mut payload = whole(&fixture, &long());
    payload["hook_event_name"] = Value::String("PreToolUse".to_owned());

    let ran = fixture.hook(&payload, &[])?;

    assert_eq!(ran.code, OK);
    assert_eq!(ran.stdout, "");
    assert_eq!(fixture.calls(), 0);
    Ok(())
}

/// The answer goes out as one line, with its Chinese as itself: the reference
/// implementation's `ensure_ascii=False`, and this text is bound for a screen.
#[test]
fn the_answer_is_one_line_of_json_with_its_chinese_intact() -> TestResult {
    let fixture = Fixture::new("wire-encoding")?;
    fixture.engine()?;

    let ran = fixture.hook(&whole(&fixture, &long()), &[])?;

    assert!(ran.stdout.ends_with('\n'), "{:?}", ran.stdout);
    assert_eq!(ran.stdout.lines().count(), 1, "{:?}", ran.stdout);
    assert!(ran.stdout.contains("润色"), "{:?}", ran.stdout);
    assert!(!ran.stdout.contains("\\u"), "{:?}", ran.stdout);
    Ok(())
}

// -- the batches of one message -------------------------------------------

/// A middle batch is the host's own to paint: no answer, and no model call
/// either, because the message it belongs to is not finished.
#[test]
fn a_middle_batch_prints_nothing_and_calls_no_engine() -> TestResult {
    let fixture = Fixture::new("batch-middle")?;
    fixture.engine()?;

    let ran = fixture.hook(&batch(&fixture, &long(), 0, false, MESSAGE), &[])?;

    assert_eq!(ran.code, OK);
    assert_eq!(ran.stdout, "");
    assert_eq!(fixture.calls(), 0);
    assert!(part(&fixture.parts(MESSAGE), 0).is_file());
    Ok(())
}

/// The final index says how many batches there were, so the model is asked
/// about the whole message and not about its last piece.
#[test]
fn the_final_index_says_how_many_batches_the_message_had() -> TestResult {
    let fixture = Fixture::new("batch-count")?;
    fixture.recording()?;
    let pieces = [LINE.repeat(7), LINE.repeat(7), LINE.repeat(6)];

    for (index, piece) in pieces.iter().enumerate() {
        let final_batch = index + 1 == pieces.len();
        let ran = fixture.hook(
            &batch(&fixture, piece, u64::try_from(index)?, final_batch, MESSAGE),
            &[],
        )?;
        assert_eq!(ran.stdout.is_empty(), !final_batch, "batch {index}");
    }

    let asked = fs::read_to_string(fixture.asked())?;
    assert!(asked.ends_with(&pieces.concat()), "{asked:?}");
    assert_eq!(fixture.calls(), 1);
    // The batches of a finished message are scratch and go the moment they are
    // assembled.
    assert!(!fixture.parts(MESSAGE).exists());
    Ok(())
}

/// `final` is the end-of-message signal whatever the delta holds, and the gap
/// is a property of the screen rather than of this batch: what an earlier batch
/// painted cannot be taken back by `displayContent`, only counted.
#[test]
fn the_gap_counts_the_newlines_an_earlier_batch_already_painted() -> TestResult {
    let fixture = Fixture::new("batch-painted")?;
    fixture.engine()?;
    let opening = format!("{}\n\n", long());

    let first = fixture.hook(&batch(&fixture, &opening, 0, false, MESSAGE), &[])?;
    let second = fixture.hook(&batch(&fixture, "", 1, true, MESSAGE), &[])?;
    let answer = second.displayed()?;

    // The empty final batch still ends the message.
    assert_eq!(first.stdout, "");
    assert!(answer.starts_with(HEADING), "{answer:?}");
    // What the reader ends up with is the batch the host painted plus this
    // answer, and that is where the one blank line has to be.
    assert_eq!(gap(&format!("{opening}{answer}"))?, BLOCK_GAP);
    Ok(())
}

/// One blank line, whatever the message happens to end on. The gap used to be
/// built by adding to whatever trailing newlines the delta happened to have,
/// which only holds still while there is exactly one of them.
#[test]
fn the_gap_above_the_block_is_one_blank_line_for_every_ending() -> TestResult {
    let fixture = Fixture::new("batch-endings")?;
    fixture.engine()?;

    for (number, ending) in ["", "\n", "\n\n", "\n\n\n"].iter().enumerate() {
        let text = format!("{}{ending}", long());
        let ran = fixture.hook(&batch(&fixture, &text, 0, true, &format!("m{number}")), &[])?;
        assert_eq!(gap(&ran.displayed()?)?, BLOCK_GAP, "ending {number}");
    }
    Ok(())
}

/// A batch index is a whole non-negative number and nothing else. JSON's
/// booleans and its numbers are separate variants here, so the reference
/// implementation's second guard — `True` is an `int` in Python — has nothing
/// to translate into; what it excluded is excluded by the type.
#[test]
fn an_index_that_is_not_a_whole_number_is_not_an_index() -> TestResult {
    let fixture = Fixture::new("batch-index")?;
    fixture.engine()?;

    for refused in [json!(true), json!(-1), json!(1.5), json!("0")] {
        let mut payload = whole(&fixture, &long());
        payload["index"] = refused.clone();
        let ran = fixture.hook(&payload, &[])?;

        assert_eq!(ran.stdout, "", "index {refused}");
        assert_eq!(fixture.calls(), 0, "index {refused}");
        assert!(!fixture.session().exists(), "index {refused}");
    }

    // The same payload with an index: the batch is cached and the message is
    // polished, so the refusals above are the index and not the payload.
    let ran = fixture.hook(&whole(&fixture, &long()), &[])?;
    assert!(ran.displayed()?.contains(HEADING));
    assert_eq!(fixture.calls(), 1);
    Ok(())
}

/// The largest index there is, is not an index: the final batch's count is
/// `index + 1`, and a number without a successor cannot say how many batches a
/// message had.
///
/// What this asserts is the fail-open result itself — nothing on screen, no
/// model call, and not so much as a state directory — and not "it did not
/// panic". The two are different observations in the two build profiles, and
/// only this one has any force in the profile we ship: `Cargo.toml` sets no
/// `[profile]` table, so a release build inherits Cargo's
/// `overflow-checks = false` and the addition wraps to a batch count of zero
/// instead of panicking, which is the quieter half of the same bug.
#[test]
fn the_largest_index_there_is_cannot_say_how_many_batches_there_were() -> TestResult {
    let fixture = Fixture::new("batch-successor")?;
    fixture.engine()?;
    let mut payload = whole(&fixture, &long());
    payload["index"] = json!(u64::MAX);

    let ran = fixture.hook(&payload, &[])?;

    assert_eq!(ran.code, OK);
    assert_eq!(ran.stdout, "");
    assert_eq!(fixture.calls(), 0);
    assert!(!fixture.session().exists());
    Ok(())
}

// -- the failures, which are all quiet on screen and none of them silent ---

/// A message with a hole in it is not polished: there is no honest rewrite of
/// one, so the turn ends the way every other failure does, and says which way
/// it failed.
#[test]
fn a_batch_that_never_arrived_is_incomplete_and_shows_the_original() -> TestResult {
    let fixture = Fixture::new("fail-incomplete")?;
    fixture.engine()?;

    let _ = fixture.hook(&batch(&fixture, &long(), 0, false, MESSAGE), &[])?;
    // Index 2 says there were three batches; the middle one never lands.
    let ran = fixture.hook(&batch(&fixture, &long(), 2, true, MESSAGE), &[])?;

    assert_eq!(ran.code, OK);
    assert_eq!(ran.stdout, "");
    assert_eq!(fixture.calls(), 0);
    assert_eq!(
        fixture.steps()?,
        [("assemble".to_owned(), "incomplete".to_owned())]
    );
    assert!(ran.stderr.contains("batches arrived"), "{:?}", ran.stderr);
    Ok(())
}

/// The contract [`crate::hook::parts::assemble`] leaves to this module: a batch
/// that is on disk and will not read back is a crash of this code, and a
/// diagnostics line calling it a missing batch would be a record that lies.
///
/// The line is the whole of the evidence — the screen looks the same either
/// way, which is what makes `crashed` against `incomplete` the only observation
/// that tells the two apart.
#[test]
fn a_batch_that_will_not_read_back_is_a_crash_and_not_a_missing_batch() -> TestResult {
    let fixture = Fixture::new("fail-unreadable")?;
    fixture.engine()?;

    let _ = fixture.hook(&batch(&fixture, &long(), 0, false, MESSAGE), &[])?;
    // On disk, present, and not text: the one failure that is neither a batch
    // that never came nor a batch that arrived.
    fs::write(part(&fixture.parts(MESSAGE), 0), [0xff, 0xfe])?;
    let ran = fixture.hook(&batch(&fixture, &long(), 1, true, MESSAGE), &[])?;

    assert_eq!(ran.code, OK);
    assert_eq!(ran.stdout, "");
    assert_eq!(fixture.calls(), 0);
    assert_eq!(
        fixture.steps()?,
        [("display".to_owned(), "crashed".to_owned())]
    );
    Ok(())
}

/// Fail-open is not fail-silent: the user gets their own text back, and the
/// session says which step failed and how.
#[test]
fn an_engine_that_fails_leaves_the_original_and_a_line_saying_how() -> TestResult {
    let fixture = Fixture::new("fail-engine")?;
    fixture.install("claude", "cat > /dev/null\nexit 1")?;
    fixture.configure("[polish]\nengine = \"claude\"\n")?;

    let ran = fixture.hook(&whole(&fixture, &long()), &[])?;

    assert_eq!(ran.code, OK);
    assert_eq!(ran.stdout, "");
    assert_eq!(
        fixture.steps()?,
        [(
            "single".to_owned(),
            FailureReason::NonzeroExit.as_str().to_owned()
        )]
    );
    Ok(())
}

/// A payload this hook cannot read is one more way to fail open, and it stops
/// before any state is created.
#[test]
fn a_payload_that_is_not_a_json_object_is_ignored() -> TestResult {
    let fixture = Fixture::new("fail-payload")?;
    fixture.engine()?;

    for refused in ["", "not json", "[]", "\"a string\"", "null"] {
        let ran = fixture.raw(refused, &[])?;
        assert_eq!(ran.code, OK, "{refused:?}");
        assert_eq!(ran.stdout, "", "{refused:?}");
    }
    assert_eq!(fixture.calls(), 0);
    assert!(!fixture.root().exists());

    // The same stdin as an object: answered. The refusals above are the shape
    // of the payload and not the fixture.
    let ran = fixture.hook(&whole(&fixture, &long()), &[])?;
    assert!(ran.displayed()?.contains(HEADING));
    Ok(())
}

// -- the entry point ------------------------------------------------------

/// Set, and the process does nothing at all — before stdin is read, so a
/// session that has switched the hook off does not even get a state directory
/// out of it.
#[test]
fn the_disable_variable_stops_the_hook_before_it_reads_stdin() -> TestResult {
    let fixture = Fixture::new("entry-disable")?;
    fixture.engine()?;
    let payload = whole(&fixture, &long());

    let off = fixture.hook(&payload, &[("LIMAE_HOOK_DISABLE", "1")])?;

    assert_eq!(off.code, OK);
    assert_eq!(off.stdout, "");
    assert_eq!(fixture.calls(), 0);
    assert!(!fixture.root().exists());

    // The same payload with the variable unset, which is what says the payload
    // was answerable all along.
    let on = fixture.hook(&payload, &[])?;
    assert!(on.displayed()?.contains(HEADING));
    assert!(fixture.root().is_dir());
    Ok(())
}

/// An empty variable is not a marker, the way an unset one is not.
#[test]
fn an_empty_disable_variable_is_not_a_disable() -> TestResult {
    let fixture = Fixture::new("entry-empty-disable")?;
    fixture.engine()?;

    let ran = fixture.hook(&whole(&fixture, &long()), &[("LIMAE_HOOK_DISABLE", "")])?;

    assert!(ran.displayed()?.contains(HEADING));
    Ok(())
}

/// The one exit code that is not `OK` belongs to a person, not to a host: a
/// hook event never passes arguments, so arguments mean somebody ran this by
/// hand and needs to be told what it wants instead.
#[test]
fn running_the_subcommand_by_hand_says_what_it_wants() -> TestResult {
    let fixture = Fixture::new("entry-usage")?;

    let ran = fixture.argv(&[MESSAGE_DISPLAY], "", &[])?;

    assert_eq!(ran.code, BAD_USAGE);
    assert_eq!(ran.stdout, "");
    assert!(ran.stderr.contains("JSON on stdin"), "{:?}", ran.stderr);
    Ok(())
}

// -- telling the two hosts' `Stop` apart ----------------------------------

/// The judgement is a string `model` beside a `last_assistant_message`, and
/// nothing else: Claude Code sends the reply too, so the reply alone cannot be
/// the test.
#[test]
fn a_stop_without_a_string_model_is_claude_s_and_not_codex_s() -> TestResult {
    let fixture = Fixture::new("host-stop")?;
    let codex_payload = codex(&fixture, &long(), &[("turn_id", "turn")]);

    assert!(is_codex_stop(&object(&codex_payload)?));
    assert!(!is_codex_stop(&object(&claude_stop())?));

    let mut numbered = codex_payload.clone();
    numbered["model"] = json!(7);
    assert!(!is_codex_stop(&object(&numbered)?));

    let mut reply_less = codex_payload;
    reply_less
        .as_object_mut()
        .ok_or("the payload builders make objects")?
        .remove("last_assistant_message");
    assert!(!is_codex_stop(&object(&reply_less)?));

    // End to end: Claude Code's `Stop` takes the other branch, which is a
    // different field of a different shape.
    pending(&fixture, "山谷")?;
    let ran = fixture.hook(&claude_stop(), &[])?;
    let answer = ran.answer()?.ok_or("a turn with a trial is answered")?;
    assert!(answer.get("systemMessage").is_none(), "{answer}");
    Ok(())
}

/// Codex has only this one event, so the pending note is consumed here: the
/// comparison and its code stay in the ledger and on screen, and the reader
/// keeps the code without this claiming the model received it.
///
/// Announcing it once is load-bearing — a `Stop` hook that always answers
/// re-triggers itself — so the second reply of the same session says nothing
/// about the trial.
#[test]
fn codex_consumes_the_pending_note_so_it_is_announced_once() -> TestResult {
    let fixture = Fixture::new("host-pending")?;
    fixture.engine()?;
    pending(&fixture, "河流")?;

    let first = fixture.hook(&codex(&fixture, &long(), &[("turn_id", "one")]), &[])?;
    let second = fixture.hook(&codex(&fixture, &long(), &[("turn_id", "two")]), &[])?;

    let said = |ran: &Ran| -> Result<String, Box<dyn Error>> {
        Ok(ran
            .answer()?
            .as_ref()
            .and_then(|answer| answer.get("systemMessage"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned())
    };
    let (first, second) = (said(&first)?, said(&second)?);

    let (block, note) = first
        .split_once("\n\n")
        .ok_or_else(|| format!("no code beside the block: {first:?}"))?;
    assert!(block.contains(HEADING), "{first:?}");
    assert!(!block.ends_with('\n'), "{first:?}");
    assert!(note.contains("河流"), "{first:?}");
    assert!(second.contains(HEADING), "{second:?}");
    assert!(!second.contains("河流"), "{second:?}");
    assert!(!fixture.session().join(PENDING_FILENAME).exists());
    Ok(())
}

/// Codex names its reply by turn, so the per-message identifier falls back to
/// the turn id: without the fallback there would be no id at all, and a Codex
/// reply would never be polished.
#[test]
fn the_message_id_falls_back_to_the_turn_id() -> TestResult {
    let fixture = Fixture::new("host-turn-id")?;
    fixture.install("claude", "cat > /dev/null\nexit 1")?;
    fixture.configure("[polish]\nengine = \"claude\"\n")?;

    let both = codex(
        &fixture,
        &long(),
        &[("turn_id", "turn-7"), ("message_id", MESSAGE)],
    );
    assert_eq!(message(&object(&both)?), MESSAGE);

    let ran = fixture.hook(&codex(&fixture, &long(), &[("turn_id", "turn-7")]), &[])?;

    assert_eq!(ran.stdout, "");
    assert_eq!(
        fixture
            .lines()?
            .iter()
            .map(|line| field(line, "message_id").to_owned())
            .collect::<Vec<_>>(),
        ["turn-7"]
    );
    Ok(())
}

// -- what the payload decides ---------------------------------------------

/// The host says which repository the session is in, and that is where the rule
/// configuration is read from — so a repository that has chosen an engine gets
/// it, and one that has disabled a rule keeps it disabled in what reaches the
/// screen.
#[test]
fn the_payload_says_which_directory_the_configuration_is_read_from() -> TestResult {
    let named = Fixture::new("config-named")?;
    named.install(
        "claude",
        &format!("cat > /dev/null\ncat <<'ANSWER'\n{REWRITE}\nANSWER"),
    )?;
    // Only where the payload points, and the process is running elsewhere.
    named.configure("[polish]\nengine = \"claude\"\n")?;
    let ran = named.hook(&whole(&named, &long()), &[])?;
    assert!(ran.displayed()?.contains(HEADING));

    // A payload that names no directory falls back to the process's own, which
    // is where the reference implementation's `Path.cwd()` lands.
    let fallback = Fixture::new("config-fallback")?;
    fallback.install(
        "claude",
        &format!("cat > /dev/null\ncat <<'ANSWER'\n{REWRITE}\nANSWER"),
    )?;
    fallback.configure_process("[polish]\nengine = \"claude\"\n")?;
    let mut payload = whole(&fallback, &long());
    payload
        .as_object_mut()
        .ok_or("the payload builders make objects")?
        .remove("cwd");
    let ran = fallback.hook(&payload, &[])?;
    assert!(ran.displayed()?.contains(HEADING));
    Ok(())
}

/// Every event sweeps the state nobody is coming back for: it is scratch, and
/// the hook is the only process that ever visits it.
#[test]
fn an_event_sweeps_the_sessions_nobody_came_back_for() -> TestResult {
    let fixture = Fixture::new("prune")?;
    fixture.engine()?;
    let old = fixture.root().join("22222222-3333-4444-5555-666666666666");
    fs::create_dir_all(&old)?;
    aged(&old, RETENTION + Duration::from_secs(60))?;

    let ran = fixture.hook(&whole(&fixture, &long()), &[])?;

    assert!(ran.displayed()?.contains(HEADING));
    assert!(!old.exists(), "a session nobody has been in for a day");
    assert!(fixture.session().is_dir(), "the live one");
    Ok(())
}

/// A payload without what its event needs is one more way to fail open, and
/// this is where the reference implementation checks it: before any state is
/// created and before any model is called.
#[test]
fn a_payload_missing_what_its_event_needs_is_left_alone() -> TestResult {
    let fixture = Fixture::new("payload-incomplete")?;
    fixture.engine()?;

    let mut headless = whole(&fixture, &long());
    headless
        .as_object_mut()
        .ok_or("the payload builders make objects")?
        .remove("session_id");
    let mut speechless = codex(&fixture, "", &[("turn_id", "turn")]);
    let mut anonymous = codex(&fixture, &long(), &[]);
    anonymous
        .as_object_mut()
        .ok_or("the payload builders make objects")?
        .remove("turn_id");
    speechless["session_id"] = Value::String(SESSION.to_owned());

    for payload in [&headless, &speechless, &anonymous] {
        let ran = fixture.hook(payload, &[])?;
        assert_eq!(ran.code, OK, "{payload}");
        assert_eq!(ran.stdout, "", "{payload}");
    }
    assert_eq!(fixture.calls(), 0);
    assert!(!fixture.root().exists());
    Ok(())
}
