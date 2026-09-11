use super::{BAD_USAGE, MESSAGE_DISPLAY, OK, serve};
use crate::hook::parts::{Limits, SIBLING_WAIT};
use crate::hook::state::{
    DIAGNOSTICS_FILENAME, ORPHAN_RETENTION, PARTS_DIRECTORY, RETENTION, STATE_DIRECTORY,
    VOID_FILENAME, instance, part,
};

use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, FileTimes};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

type TestResult = Result<(), Box<dyn Error>>;

/// A fixed clock reading, so that no assertion here depends on the wall clock.
fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

/// The ids the host sends, in the shape it sends them: UUIDs, which survive
/// sanitising unchanged.
const SESSION: &str = "11111111-2222-3333-4444-555555555555";
const MESSAGE: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
const TURN: &str = "99999999-8888-7777-6666-555555555555";

/// One line of synthetic Chinese prose with a halfwidth comma in it, and what
/// the rules make of it. Every arm that needs "a batch the fixes change" uses
/// these, so that what changes is the one thing the arm is about.
const LINE: &str = "ACME 的报告写得不好,请把它改得像人话一些。\n";
const FIXED: &str = "ACME 的报告写得不好，请把它改得像人话一些。\n";

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

/// One run's four directories: a home nothing real lives in, the scratch
/// directory the state root is derived from, the directory the payload names,
/// and the process's own working directory.
///
/// The last two are deliberately different, so that a test which configures a
/// rule in `cwd` is also saying the payload is what decides where the
/// configuration is read from.
///
/// No assertion here depends on what this machine has installed or on what the
/// user running the suite has configured.
struct Fixture {
    home: TempDir,
    scratch: TempDir,
    cwd: TempDir,
    elsewhere: TempDir,
    /// What a message is held to; the defaults unless a test is about them.
    limits: Limits,
}

impl Fixture {
    fn new(name: &str) -> Result<Self, std::io::Error> {
        Ok(Self {
            home: TempDir::new(&format!("{name}-home"))?,
            scratch: TempDir::new(&format!("{name}-scratch"))?,
            cwd: TempDir::new(&format!("{name}-cwd"))?,
            elsewhere: TempDir::new(&format!("{name}-elsewhere"))?,
            limits: Limits::DEFAULT,
        })
    }

    /// The same, with a wait short enough that an arm about a batch that never
    /// comes does not sit through the real one.
    fn impatient(name: &str) -> Result<Self, std::io::Error> {
        let mut fixture = Self::new(name)?;
        fixture.limits.wait = Duration::from_millis(50);
        Ok(fixture)
    }

    /// The environment of a run, plus whatever a test sets.
    fn with(&self, extra: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        let mut env: Vec<(OsString, OsString)> = [
            ("HOME", self.home.path().as_os_str().to_owned()),
            // Where scratch goes is where the state goes: the hook has no
            // setting of its own for it, on purpose.
            ("TMPDIR", self.scratch.path().as_os_str().to_owned()),
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

    /// Write the rule configuration where the payload says to look for it.
    fn configure(&self, table: &str) -> TestResult {
        fs::write(self.cwd.path().join("limae.toml"), table)?;
        Ok(())
    }

    fn root(&self) -> PathBuf {
        self.scratch.path().join(STATE_DIRECTORY)
    }

    fn session(&self) -> PathBuf {
        self.root().join(SESSION)
    }

    fn parts(&self, message: &str, turn: &str) -> PathBuf {
        self.session()
            .join(PARTS_DIRECTORY)
            .join(instance(message, turn))
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

    /// Run one `MessageDisplay` batch of the usual message and return what the
    /// host would show in its place — empty when the hook said nothing.
    fn shown(&self, delta: &str, index: u64, is_final: bool) -> Result<String, Box<dyn Error>> {
        self.hook(&batch(self, delta, index, is_final), &[])?
            .displayed()
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
        let code = serve(
            &args,
            self.elsewhere.path(),
            &self.with(extra),
            &mut input,
            &mut stdout,
            &mut stderr,
            now(),
            &self.limits,
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

/// One `MessageDisplay` batch, as Claude Code sends it, of the usual message.
fn batch(fixture: &Fixture, delta: &str, index: u64, is_final: bool) -> Value {
    batch_of(fixture, delta, index, is_final, MESSAGE, TURN)
}

/// One `MessageDisplay` batch of the message and turn named.
fn batch_of(
    fixture: &Fixture,
    delta: &str,
    index: u64,
    is_final: bool,
    message: &str,
    turn: &str,
) -> Value {
    json!({
        "session_id": SESSION,
        "transcript_path": "/dev/null",
        "cwd": fixture.cwd.path().to_string_lossy(),
        "hook_event_name": MESSAGE_DISPLAY,
        "prompt_id": "prompt",
        "turn_id": turn,
        "message_id": message,
        "index": index,
        "final": is_final,
        "delta": delta,
    })
}

/// Feed one message through the hook the way the host does — every batch a
/// run of its own, in order — and put the answers back together, taking the
/// batch itself wherever the hook said nothing.
///
/// Middle batches are the whole lines given; the final batch is `tail`, which
/// is what follows the message's last line feed and may be empty.
fn streamed(fixture: &Fixture, middles: &[&str], tail: &str) -> Result<String, Box<dyn Error>> {
    let mut screen = String::new();
    for (index, delta) in middles.iter().enumerate() {
        assert!(delta.ends_with('\n'), "a middle batch ends on a line feed");
        let shown = fixture.shown(delta, u64::try_from(index)?, false)?;
        screen.push_str(if shown.is_empty() { delta } else { &shown });
    }
    let shown = fixture.shown(tail, u64::try_from(middles.len())?, true)?;
    screen.push_str(if shown.is_empty() { tail } else { &shown });
    Ok(screen)
}

/// The whole message, fixed at once, the way `limae --fix` fixes it.
fn whole(text: &str) -> Result<String, Box<dyn Error>> {
    Ok(crate::pipeline::Pipeline::new()?.fix(text, &crate::config::ResolvedConfig::default())?)
}

// -- the wire shape -------------------------------------------------------

/// A `MessageDisplay` answer replaces the batch, and does it through the field
/// that host reads. Asserting only that some JSON was printed cannot tell this
/// event's shape from a `Stop`'s.
#[test]
fn a_display_batch_answers_through_display_content() -> TestResult {
    let fixture = Fixture::new("shape-display")?;

    let ran = fixture.hook(&batch(&fixture, LINE, 0, false), &[])?;

    assert_eq!(ran.code, OK);
    let answer = ran.answer()?.ok_or("no answer")?;
    assert_eq!(
        answer,
        json!({
            "hookSpecificOutput": {
                "hookEventName": MESSAGE_DISPLAY,
                "displayContent": FIXED,
            }
        })
    );
    Ok(())
}

/// The answer goes out as one line, with its Chinese as itself: this text is
/// bound for a screen.
#[test]
fn the_answer_is_one_line_of_json_with_its_chinese_intact() -> TestResult {
    let fixture = Fixture::new("shape-line")?;

    let ran = fixture.hook(&batch(&fixture, LINE, 0, true), &[])?;

    assert_eq!(ran.stdout.matches('\n').count(), 1);
    assert!(ran.stdout.ends_with('\n'));
    assert!(
        ran.stdout.contains("请把它改得像人话一些"),
        "{}",
        ran.stdout
    );
    assert!(!ran.stdout.contains("\\u"), "{}", ran.stdout);
    Ok(())
}

/// `Stop` — Claude Code's, and the Codex one that used to be answered — is
/// received and left alone: no JSON at all, exit 0, and no state either. The
/// display arm beside them is what says the silence is the event's and not the
/// fixture's.
#[test]
fn a_stop_from_either_host_prints_nothing_at_all() -> TestResult {
    let fixture = Fixture::new("shape-stop")?;
    let claude = json!({
        "session_id": SESSION,
        "transcript_path": "/dev/null",
        "cwd": fixture.cwd.path().to_string_lossy(),
        "hook_event_name": "Stop",
        "stop_hook_active": false,
        "last_assistant_message": LINE,
    });
    let codex = json!({
        "session_id": SESSION,
        "cwd": fixture.cwd.path().to_string_lossy(),
        "hook_event_name": "Stop",
        "turn_id": TURN,
        "model": "gpt-5.6-terra",
        "last_assistant_message": LINE,
    });

    for payload in [&claude, &codex] {
        let ran = fixture.hook(payload, &[])?;
        assert_eq!((ran.code, ran.stdout.as_str()), (OK, ""), "{payload}");
    }
    assert!(!fixture.root().exists());

    assert_eq!(fixture.shown(LINE, 0, true)?, FIXED);
    Ok(())
}

// -- batch by batch equals the whole -------------------------------------

/// The shape of ADR-0016 读数 B: a fence with the two forms the line-by-line
/// fix got wrong in it — `status.success()` and `..limits() }` — and prose
/// with violations around it, cut into batches of one line and of several.
/// What reaches the screen batch by batch is what `limae --fix` makes of the
/// whole, byte for byte.
#[test]
fn a_message_with_a_fence_in_it_reaches_the_screen_as_its_whole_fix() -> TestResult {
    let fixture = Fixture::new("whole-fence")?;
    let middles = [
        "测试通过了,看这段:\n",
        "\n```rust\nassert!(result.status.success());\n",
        "let limits = EngineLimits { timeout, ..limits() };\n```\n",
        "\n收尾(见上)。\n",
    ];
    let tail = "完毕(无尾换行)";
    let text = format!("{}{tail}", middles.concat());

    let screen = streamed(&fixture, &middles, tail)?;

    assert_eq!(screen, whole(&text)?);
    assert!(screen.contains("status.success());\n"), "{screen}");
    assert!(screen.contains("..limits() };\n"), "{screen}");
    assert!(screen.contains("测试通过了，看这段："), "{screen}");
    assert!(screen.contains("收尾 (见上)。"), "{screen}");
    assert!(screen.ends_with("完毕 (无尾换行)"), "{screen}");
    assert_eq!(fixture.steps()?, []);
    Ok(())
}

/// A directive in one batch governs a line in a later one, because the batch
/// is fixed with the directive in its prefix: `disable-next-line` over a batch
/// boundary, and a `disable` … `enable` range that opens and closes in
/// different batches.
#[test]
fn a_directive_in_an_earlier_batch_governs_the_lines_after_it() -> TestResult {
    let fixture = Fixture::new("whole-directive")?;
    let middles = [
        "<!-- limae-disable-next-line zh-typography-1 -->\n",
        "第一行,不修。\n",
        "<!-- limae-disable -->\n\n第二行,不修。\n",
        "\n<!-- limae-enable -->\n",
    ];
    let tail = "第三行,修。";
    let text = format!("{}{tail}", middles.concat());

    let screen = streamed(&fixture, &middles, tail)?;

    assert_eq!(screen, whole(&text)?);
    assert!(screen.contains("第一行,不修。"), "{screen}");
    assert!(screen.contains("第二行,不修。"), "{screen}");
    assert!(screen.ends_with("第三行，修。"), "{screen}");
    assert_eq!(fixture.steps()?, []);
    Ok(())
}

/// Middle batches keep their line feed and the final batch keeps its lack of
/// one; a final batch that is empty — the message ended on a line feed — gets
/// no answer at all.
#[test]
fn the_ending_of_every_answer_is_the_ending_of_its_batch() -> TestResult {
    let fixture = Fixture::new("endings")?;

    let middle = fixture.shown(LINE, 0, false)?;
    let last = fixture.shown(LINE.trim_end(), 1, true)?;

    assert_eq!(middle, FIXED);
    assert_eq!(last, FIXED.trim_end());

    // A message that ends on a line feed: its final batch is empty.
    let other = "bbbbbbbb-bbbb-cccc-dddd-eeeeeeeeeeee";
    let ran = fixture.hook(&batch_of(&fixture, LINE, 0, false, other, TURN), &[])?;
    assert_eq!(ran.displayed()?, FIXED);
    let ran = fixture.hook(&batch_of(&fixture, "", 1, true, other, TURN), &[])?;
    assert_eq!((ran.code, ran.stdout.as_str()), (OK, ""));
    assert_eq!(fixture.steps()?, []);
    Ok(())
}

// -- the batches before this one -----------------------------------------

/// A batch before this one that never lands: this one is shown as it came,
/// and the session says so. Once the batch lands, the same one is answered.
#[test]
fn a_batch_waits_for_the_ones_before_it_and_gives_up_without_them() -> TestResult {
    let fixture = Fixture::impatient("siblings-missing")?;

    let ran = fixture.hook(&batch(&fixture, LINE, 1, false), &[])?;

    assert_eq!((ran.code, ran.stdout.as_str()), (OK, ""));
    assert!(ran.stderr.contains("0/1 earlier batches"), "{}", ran.stderr);
    assert_eq!(
        fixture.steps()?,
        [("siblings".to_owned(), "incomplete".to_owned())]
    );

    // Batch 0 lands, and batch 1 comes round again — the host does not do
    // this, but it is the arm that says the refusal was about the hole.
    assert_eq!(fixture.shown("甲。\n", 0, false)?, "");
    assert_eq!(fixture.shown(LINE, 1, false)?, FIXED);
    assert_eq!(fixture.steps()?.len(), 1);
    Ok(())
}

/// The turn is part of the key. A batch left behind under the same message id
/// in another turn — ordinary prose at index 1, where this message has its
/// fence opener — is not this message's, so index 2 finds a hole and waits,
/// rather than reading the prose as its prefix and fixing its code.
#[test]
fn a_batch_from_another_turn_is_not_this_message_s_prefix() -> TestResult {
    let fixture = Fixture::impatient("siblings-turn")?;
    let old = "old-turn";
    crate::hook::state::keep(&fixture.parts(MESSAGE, old), 1, "普通正文。\n")?;

    assert_eq!(fixture.shown("开头。\n", 0, false)?, "");
    let ran = fixture.hook(&batch(&fixture, "打印(x)\n", 2, false), &[])?;

    assert_eq!((ran.code, ran.stdout.as_str()), (OK, ""));
    assert_eq!(
        fixture.steps()?,
        [("siblings".to_owned(), "incomplete".to_owned())]
    );
    assert!(fixture.parts(MESSAGE, old).is_dir());
    assert!(fixture.parts(MESSAGE, TURN).is_dir());
    Ok(())
}

/// The same index sent twice with the same content changes nothing; with
/// different content the instance is abandoned — this batch and every later
/// one shown as they came, each saying so — and nothing is overwritten.
#[test]
fn a_batch_resent_the_same_is_idempotent_and_resent_different_abandons_the_message() -> TestResult {
    let fixture = Fixture::new("siblings-resend")?;

    assert_eq!(fixture.shown(LINE, 0, false)?, FIXED);
    assert_eq!(fixture.shown(LINE, 0, false)?, FIXED);
    assert_eq!(fixture.steps()?, []);

    assert_eq!(fixture.shown("另一条,消息。\n", 0, false)?, "");
    assert_eq!(fixture.shown(LINE, 1, false)?, "");
    assert_eq!(fixture.shown(LINE, 2, true)?, "");

    assert_eq!(
        fixture.steps()?,
        [
            ("siblings".to_owned(), "incomplete".to_owned()),
            ("siblings".to_owned(), "incomplete".to_owned()),
            ("siblings".to_owned(), "incomplete".to_owned()),
        ]
    );
    let parts = fixture.parts(MESSAGE, TURN);
    assert!(parts.join(VOID_FILENAME).is_file());
    assert_eq!(fs::read_to_string(part(&parts, 0))?, LINE);
    Ok(())
}

/// The two size limits, each with a message that just fits and one that does
/// not. Past a limit the instance is abandoned rather than the prefix cut
/// short: the batch of code inside a fence that follows must come back as it
/// came, which a replay over a truncated prefix would fix as prose.
#[test]
fn a_message_past_a_limit_is_left_alone_from_there_on_prefix_and_all() -> TestResult {
    let opener = "```\n";
    let code = "打印(x)\n";

    // Batches: two fit, a third does not.
    let mut fixture = Fixture::new("limits-batches")?;
    fixture.limits.batches = 2;
    assert_eq!(fixture.shown(LINE, 0, false)?, FIXED);
    assert_eq!(fixture.shown(LINE, 1, false)?, FIXED);
    assert_eq!(fixture.steps()?, []);
    let mut over = Fixture::new("limits-batches-over")?;
    over.limits.batches = 2;
    assert_eq!(over.shown(opener, 0, false)?, "");
    assert_eq!(over.shown(LINE, 1, false)?, "");
    assert_eq!(over.shown(code, 2, false)?, "");
    assert_eq!(over.shown(code, 3, true)?, "");
    assert_eq!(
        over.steps()?,
        [
            ("siblings".to_owned(), "incomplete".to_owned()),
            ("siblings".to_owned(), "incomplete".to_owned()),
        ]
    );

    // Bytes: the opener and one line fit exactly, a second line does not.
    let mut fixture = Fixture::new("limits-bytes")?;
    fixture.limits.bytes = opener.len() + LINE.len();
    assert_eq!(fixture.shown(opener, 0, false)?, "");
    assert_eq!(fixture.shown(LINE, 1, false)?, "");
    assert_eq!(fixture.steps()?, []);
    let mut over = Fixture::new("limits-bytes-over")?;
    over.limits.bytes = opener.len() + LINE.len();
    assert_eq!(over.shown(opener, 0, false)?, "");
    assert_eq!(over.shown(LINE, 1, false)?, "");
    assert_eq!(over.shown(code, 2, false)?, "");
    assert_eq!(over.shown(code, 3, true)?, "");
    assert_eq!(
        over.steps()?,
        [
            ("siblings".to_owned(), "incomplete".to_owned()),
            ("siblings".to_owned(), "incomplete".to_owned()),
        ]
    );
    // The control for the "as it came": the same code with no fence and no
    // limit is prose, and is fixed.
    let plain = Fixture::new("limits-control")?;
    assert_eq!(plain.shown(code, 0, false)?, "打印 (x)\n");
    Ok(())
}

// -- the two fail-opens on one batch -------------------------------------

/// A middle batch that ends mid-line, the batch that completes that line, and
/// the batch after: the first two are shown as they came and say so, the third
/// is fixed. The control arm puts the line feed back and all three are fixed.
#[test]
fn a_batch_boundary_inside_a_line_is_partial_for_that_line_and_no_further() -> TestResult {
    let fixture = Fixture::new("partial")?;

    assert_eq!(fixture.shown("ACME 的报告写得不好,", 0, false)?, "");
    assert_eq!(fixture.shown("请把它改得像人话一些。\n", 1, false)?, "");
    assert_eq!(fixture.shown(LINE, 2, false)?, FIXED);
    assert_eq!(
        fixture.steps()?,
        [
            ("siblings".to_owned(), "partial".to_owned()),
            ("siblings".to_owned(), "partial".to_owned()),
        ]
    );

    let control = Fixture::new("partial-control")?;
    assert_eq!(
        control.shown("ACME 的报告写得不好,\n", 0, false)?,
        "ACME 的报告写得不好，\n"
    );
    assert_eq!(control.shown("请把它改得像人话一些。\n", 1, false)?, "");
    assert_eq!(control.shown(LINE, 2, false)?, FIXED);
    assert_eq!(control.steps()?, []);
    Ok(())
}

/// `spec/fixtures/span-across-line-break` cut where the span crosses the line.
/// The opening batch goes up as it came and says why; the closing batch keeps
/// the code inside the span and fixes the prose after it. The control arm
/// sends both lines in one batch, and nothing is declined.
#[test]
fn a_batch_that_may_be_inside_a_code_span_is_left_until_the_span_is_decided() -> TestResult {
    let fixture = Fixture::new("unclosed")?;
    let opening = "`你好,世界\n";
    let closing = "函数(x)` 后文(y)";

    assert_eq!(fixture.shown(opening, 0, false)?, "");
    assert_eq!(fixture.shown(closing, 1, true)?, "函数(x)` 后文 (y)");
    assert_eq!(
        fixture.steps()?,
        [("siblings".to_owned(), "unclosed".to_owned())]
    );

    let control = Fixture::new("unclosed-control")?;
    assert_eq!(
        control.shown(&format!("{opening}{closing}"), 0, true)?,
        "`你好,世界\n函数(x)` 后文 (y)"
    );
    assert_eq!(control.steps()?, []);
    Ok(())
}

// -- the configuration ---------------------------------------------------

/// The host says which repository the session is in, and that is where the
/// rule configuration is read from: a rule the repository has disabled stays
/// disabled on screen, and a configuration that will not read leaves the batch
/// alone and says so — rather than quietly fixing under the defaults, which
/// would look the same as the configuration having taken.
#[test]
fn the_payload_says_which_directory_the_configuration_is_read_from() -> TestResult {
    let fixture = Fixture::new("config")?;

    assert_eq!(fixture.shown(LINE, 0, false)?, FIXED);

    fixture.configure("disable = [\"zh-typography-1\"]\n")?;
    assert_eq!(fixture.shown(LINE, 1, false)?, "");
    assert_eq!(fixture.steps()?, []);

    fixture.configure("disable = [\n")?;
    assert_eq!(fixture.shown(LINE, 2, false)?, "");
    assert_eq!(fixture.steps()?, [("fix".to_owned(), "config".to_owned())]);

    // Written where the process runs instead: not where the payload points,
    // so not read.
    fs::remove_file(fixture.cwd.path().join("limae.toml"))?;
    fs::write(
        fixture.elsewhere.path().join("limae.toml"),
        "disable = [\"zh-typography-1\"]\n",
    )?;
    assert_eq!(fixture.shown(LINE, 3, true)?, FIXED);
    Ok(())
}

/// Nothing under the home directory: no user-level configuration is read,
/// created or looked for (ADR-0016 section 三 leaves that open, and open means
/// not done).
#[test]
fn a_batch_leaves_the_home_directory_untouched() -> TestResult {
    let fixture = Fixture::new("home")?;

    assert_eq!(fixture.shown(LINE, 0, true)?, FIXED);

    assert_eq!(fs::read_dir(fixture.home.path())?.count(), 0);
    Ok(())
}

// -- the state left behind -----------------------------------------------

/// Every event sweeps the state nobody is coming back for: a session nobody
/// has been in for a day, and inside a live session the batches of a message
/// an hour old — whether it finished or was interrupted, since neither deletes
/// its own batches. The message just streamed keeps its own.
#[test]
fn an_event_sweeps_the_sessions_and_the_messages_nobody_came_back_for() -> TestResult {
    let fixture = Fixture::new("prune")?;
    let old = fixture.root().join("22222222-3333-4444-5555-666666666666");
    fs::create_dir_all(&old)?;
    aged(&old, RETENTION + Duration::from_secs(60))?;
    // An interrupted message: every batch `final: false`, then nothing.
    fixture.hook(
        &batch_of(&fixture, LINE, 0, false, "interrupted", TURN),
        &[],
    )?;
    let interrupted = fixture.parts("interrupted", TURN);
    assert!(interrupted.is_dir());
    aged(&interrupted, ORPHAN_RETENTION + Duration::from_secs(60))?;

    assert_eq!(fixture.shown(LINE, 0, true)?, FIXED);

    assert!(!old.exists(), "a session nobody has been in for a day");
    assert!(!interrupted.exists(), "a message an hour old");
    assert!(
        fixture.parts(MESSAGE, TURN).is_dir(),
        "the one just streamed"
    );
    Ok(())
}

/// A finished message leaves its batches too: the final batch does not delete
/// them, because a batch before it may still be reading them.
#[test]
fn the_final_batch_leaves_the_message_s_batches_for_the_sweep() -> TestResult {
    let fixture = Fixture::new("final-leaves")?;

    assert_eq!(fixture.shown(LINE, 0, false)?, FIXED);
    assert_eq!(fixture.shown(LINE.trim_end(), 1, true)?, FIXED.trim_end());

    let parts = fixture.parts(MESSAGE, TURN);
    assert_eq!(fs::read_to_string(part(&parts, 0))?, LINE);
    assert_eq!(fs::read_to_string(part(&parts, 1))?, LINE.trim_end());
    Ok(())
}

// -- the failures, which are all quiet on screen and none of them silent ---

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

    assert_eq!(fixture.shown(LINE, 0, false)?, FIXED);
    // On disk, present, and not text: the one failure that is neither a batch
    // that never came nor a batch that arrived.
    fs::write(part(&fixture.parts(MESSAGE, TURN), 0), [0xff, 0xfe])?;
    let ran = fixture.hook(&batch(&fixture, LINE, 1, true), &[])?;

    assert_eq!((ran.code, ran.stdout.as_str()), (OK, ""));
    assert_eq!(
        fixture.steps()?,
        [("display".to_owned(), "crashed".to_owned())]
    );
    Ok(())
}

/// A payload this hook cannot read is one more way to fail open, and it stops
/// before any state is created.
#[test]
fn a_payload_that_is_not_a_json_object_is_ignored() -> TestResult {
    let fixture = Fixture::new("fail-payload")?;

    for refused in ["", "not json", "[]", "\"a string\"", "null"] {
        let ran = fixture.raw(refused, &[])?;
        assert_eq!((ran.code, ran.stdout.as_str()), (OK, ""), "{refused:?}");
    }
    assert!(!fixture.root().exists());

    // The same stdin as an object: answered. The refusals above are the shape
    // of the payload and not the fixture.
    assert_eq!(fixture.shown(LINE, 0, true)?, FIXED);
    Ok(())
}

/// A payload without what its event needs is left alone before any state is
/// created: the session, the message and the turn are each half of a path,
/// and the delta is the thing to fix.
#[test]
fn a_payload_missing_what_its_event_needs_is_left_alone() -> TestResult {
    let fixture = Fixture::new("payload-incomplete")?;

    for missing in ["session_id", "message_id", "turn_id", "delta", "index"] {
        let mut payload = batch(&fixture, LINE, 0, true);
        payload
            .as_object_mut()
            .ok_or("the payload builder makes objects")?
            .remove(missing);
        let ran = fixture.hook(&payload, &[])?;
        assert_eq!((ran.code, ran.stdout.as_str()), (OK, ""), "{missing}");
    }
    assert!(!fixture.root().exists());

    assert_eq!(fixture.shown(LINE, 0, true)?, FIXED);
    Ok(())
}

/// A batch index is a whole non-negative number and nothing else. JSON's
/// booleans and its numbers are separate variants here, so `true` is not `1`.
#[test]
fn an_index_that_is_not_a_whole_number_is_not_an_index() -> TestResult {
    let fixture = Fixture::new("batch-index")?;

    for refused in [json!(true), json!(-1), json!(1.5), json!("0")] {
        let mut payload = batch(&fixture, LINE, 0, true);
        payload["index"] = refused.clone();
        let ran = fixture.hook(&payload, &[])?;
        assert_eq!((ran.code, ran.stdout.as_str()), (OK, ""), "index {refused}");
        assert!(!fixture.session().exists(), "index {refused}");
    }

    assert_eq!(fixture.shown(LINE, 0, true)?, FIXED);
    Ok(())
}

// -- the entry point ------------------------------------------------------

/// Set, and the process does nothing at all — before stdin is read, so a
/// session that has switched the hook off does not even get a state directory
/// out of it. An empty variable is not a marker, the way an unset one is not.
#[test]
fn the_disable_variable_stops_the_hook_before_it_reads_stdin() -> TestResult {
    let fixture = Fixture::new("entry-disable")?;
    let payload = batch(&fixture, LINE, 0, true);

    let off = fixture.hook(&payload, &[("LIMAE_HOOK_DISABLE", "1")])?;
    assert_eq!((off.code, off.stdout.as_str()), (OK, ""));
    assert!(!fixture.root().exists());

    let on = fixture.hook(&payload, &[("LIMAE_HOOK_DISABLE", "")])?;
    assert_eq!(on.displayed()?, FIXED);
    Ok(())
}

/// The one exit code that is not `OK` belongs to a person, not to a host: a
/// hook event never passes arguments, so arguments mean somebody ran this by
/// hand and needs to be told what it wants instead.
#[test]
fn running_the_subcommand_by_hand_says_what_it_wants() -> TestResult {
    let fixture = Fixture::new("entry-usage")?;

    let ran = fixture.argv(&["MessageDisplay"], "", &[])?;

    assert_eq!((ran.code, ran.stdout.as_str()), (BAD_USAGE, ""));
    assert!(
        ran.stderr.contains("reads one hook event as JSON on stdin"),
        "{}",
        ran.stderr
    );
    assert!(!fixture.root().exists());
    Ok(())
}

/// The real wait is what a hook event runs under, and the fixture's shortened
/// one is a test's own: the two must not be confused, so the default is
/// spelled out here beside the fixture that departs from it.
#[test]
fn a_hook_event_runs_under_the_default_limits() {
    assert_eq!(Limits::DEFAULT.wait, SIBLING_WAIT);
}
