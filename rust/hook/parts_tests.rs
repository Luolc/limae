use super::{
    Limits, Outcome, SIBLING_POLL, SIBLING_WAIT, Stored, assemble, replay, replay_with, store,
};
use crate::config::{CliOverrides, ResolvedConfig, resolve};
use crate::hook::state::{self, DIAGNOSTICS_FILENAME, Kind, Step};
use crate::pipeline::Pipeline;

use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type TestResult = Result<(), Box<dyn Error>>;

/// A fixed clock reading, so that no assertion here depends on the wall clock.
fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "limae-hook-parts-{name}-{}-{}",
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

/// Write one batch the way [`state::keep`] does, so the reader under test is
/// reading what the writer under test writes.
fn cache(parts: &Path, index: usize, delta: &str) -> TestResult {
    state::keep(parts, index, delta)?;
    Ok(())
}

fn diagnostics(directory: &Path) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    fs::read_to_string(directory.join(DIAGNOSTICS_FILENAME))?
        .lines()
        .map(|line| Ok(serde_json::from_str(line)?))
        .collect()
}

// -- putting a message back together --------------------------------------

/// The batches are written here in an order no reader may depend on, because
/// the directory hands them back in an order no reader may depend on either:
/// what puts the message back in the order the user read it is the index in the
/// name, and nothing else. Joining them in the order the directory lists them
/// turns this red on most filesystems and is not caught by asserting the whole
/// is non-empty.
#[test]
fn assemble_joins_every_batch_in_index_order() -> TestResult {
    let session = TempDir::new("assemble")?;
    let parts = session.path().join("parts/message");
    for (index, delta) in [(2, "第三段。\n"), (0, "第一段。\n"), (1, "第二段。\n")] {
        cache(&parts, index, delta)?;
    }
    let mut stderr = Vec::new();

    let whole = assemble(
        &parts,
        3,
        Instant::now() + SIBLING_WAIT,
        session.path(),
        "message",
        now(),
        &mut stderr,
    )?;

    assert_eq!(whole.as_deref(), Some("第一段。\n第二段。\n第三段。\n"));
    assert_eq!(stderr, b"");
    assert!(!session.path().join(DIAGNOSTICS_FILENAME).exists());
    Ok(())
}

/// The wait is the point: the host starts one process per batch without waiting
/// for it, so a batch can be here before the one before it is. The deadline is
/// [`SIBLING_WAIT`] from now, as the caller sets it, and the sibling lands well
/// inside it — a wait shorter than the sleep below turns this red.
#[test]
fn assemble_waits_for_a_sibling_that_is_still_being_written() -> TestResult {
    let session = TempDir::new("assemble-wait")?;
    let parts = session.path().join("parts/message");
    cache(&parts, 0, "第一段。\n")?;
    let late = parts.clone();
    let writer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        state::keep(&late, 1, "第二段。\n")
    });
    let mut stderr = Vec::new();

    let whole = assemble(
        &parts,
        2,
        Instant::now() + SIBLING_WAIT,
        session.path(),
        "message",
        now(),
        &mut stderr,
    )?;

    writer.join().map_err(|_| "the writing thread panicked")??;
    assert_eq!(whole.as_deref(), Some("第一段。\n第二段。\n"));
    assert_eq!(stderr, b"");
    Ok(())
}

/// A prefix with a hole in it: three batches come before this one, two of them
/// are on disk, and the third never comes. A prefix with a hole carries none of
/// the state the replay is for, so this batch ends the way every other failure
/// does — and says so three ways.
#[test]
fn assemble_gives_up_on_a_message_that_is_still_missing_a_batch() -> TestResult {
    let session = TempDir::new("assemble-hole")?;
    let parts = session.path().join("parts/message");
    cache(&parts, 0, "第一段。\n")?;
    cache(&parts, 2, "第三段。\n")?;
    let mut stderr = Vec::new();

    let started = Instant::now();
    let whole = assemble(
        &parts,
        3,
        started + Duration::from_millis(150),
        session.path(),
        "message",
        now(),
        &mut stderr,
    )?;
    let waited = started.elapsed();

    assert_eq!(whole, None);
    assert!(
        waited >= Duration::from_millis(150),
        "gave up after {waited:?}"
    );
    assert_eq!(
        String::from_utf8(stderr)?,
        "limae hook: 2/3 earlier batches arrived before the deadline; showing this one as it came\n"
    );
    let written = diagnostics(session.path())?;
    assert_eq!(written.len(), 1);
    assert_eq!(written[0]["message_id"], "message");
    assert_eq!(written[0]["step"], "siblings");
    assert_eq!(written[0]["kind"], "incomplete");
    Ok(())
}

/// The count in that line is of what arrived, not of what was asked for: it is
/// the one number worth saying, and the batches themselves are the user's own
/// text and stay out of every log.
#[test]
fn the_line_about_a_hole_counts_the_batches_and_quotes_none_of_them() -> TestResult {
    let session = TempDir::new("assemble-count")?;
    let parts = session.path().join("parts/message");
    cache(&parts, 0, "机密的第一段。\n")?;
    let mut stderr = Vec::new();

    let whole = assemble(
        &parts,
        4,
        Instant::now(),
        session.path(),
        "message",
        now(),
        &mut stderr,
    )?;
    let line = String::from_utf8(stderr)?;

    assert_eq!(whole, None);
    assert!(
        line.starts_with("limae hook: 1/4 earlier batches"),
        "{line:?}"
    );
    assert!(!line.contains("机密"), "{line:?}");
    Ok(())
}

/// Reading a batch back can fail, and that is the caller's to fail open on the
/// way it fails open on everything else — not something this quietly reports as
/// a hole, which would say a batch never arrived when one did.
#[test]
fn assemble_reports_a_batch_it_cannot_read_rather_than_calling_it_missing() -> TestResult {
    let session = TempDir::new("assemble-unreadable")?;
    let parts = session.path().join("parts/message");
    cache(&parts, 0, "第一段。\n")?;
    let mut file = fs::File::create(state::part(&parts, 1))?;
    file.write_all(&[0xff, 0xfe])?;
    drop(file);
    let mut stderr = Vec::new();

    let failed = assemble(
        &parts,
        2,
        Instant::now() + SIBLING_WAIT,
        session.path(),
        "message",
        now(),
        &mut stderr,
    );

    assert!(failed.is_err(), "an unreadable batch is not a missing one");
    Ok(())
}

/// What this costs is set by the batches on disk, never by the number the host
/// sent — and the number the host sent is the one thing here an outside caller
/// picks. `usize::MAX` is how that is made observable rather than argued: no
/// implementation that lays out, visits, or otherwise spends anything per batch
/// asked for can return from this call at all, so this test finishing at all is
/// the assertion. Laying the paths out one per batch — what this code did until
/// the fix — cannot even allocate that many and takes the process down with a
/// capacity overflow, which is a non-zero exit out of a hook event and the
/// breach of ADR-0016 section 一 this is about; visiting them one at a time
/// without allocating runs past any deadline this suite would tolerate.
///
/// Timing is deliberately not asserted: a duration is both brittle and, on a
/// fast enough machine, green for an implementation that is still linear in the
/// host's number.
#[test]
fn assemble_costs_what_is_on_disk_and_not_what_the_host_asked_for() -> TestResult {
    let session = TempDir::new("assemble-unbounded")?;
    let parts = session.path().join("parts/message");
    cache(&parts, 0, "第一段。\n")?;
    cache(&parts, 1, "第二段。\n")?;
    let mut stderr = Vec::new();

    let whole = assemble(
        &parts,
        usize::MAX,
        Instant::now(),
        session.path(),
        "message",
        now(),
        &mut stderr,
    )?;

    assert_eq!(whole, None);
    assert_eq!(
        String::from_utf8(stderr)?,
        format!(
            "limae hook: 2/{} earlier batches arrived before the deadline; showing this one as it came\n",
            usize::MAX
        )
    );
    Ok(())
}

/// A batch after this one has landed already, so the directory holds as many
/// files as the prefix has batches and is still missing one of them. Counting
/// them would call the prefix whole and then fall over reading a batch that is
/// not there; what settles it is which indices are present.
#[test]
fn assemble_is_not_fooled_by_a_stale_batch_that_makes_the_count_come_out_right() -> TestResult {
    let session = TempDir::new("assemble-stale")?;
    let parts = session.path().join("parts/message");
    cache(&parts, 0, "第一段。\n")?;
    cache(&parts, 5, "后面的一批。\n")?;
    let mut stderr = Vec::new();

    let whole = assemble(
        &parts,
        2,
        Instant::now(),
        session.path(),
        "message",
        now(),
        &mut stderr,
    )?;

    assert_eq!(whole, None);
    assert_eq!(
        String::from_utf8(stderr)?,
        "limae hook: 1/2 earlier batches arrived before the deadline; showing this one as it came\n"
    );
    let written = diagnostics(session.path())?;
    assert_eq!(written.len(), 1);
    assert_eq!(written[0]["kind"], "incomplete");
    Ok(())
}

/// The same later batch, once the prefix has all landed: it is not part of the
/// prefix and not read into it.
#[test]
fn assemble_leaves_a_stale_batch_out_of_a_message_that_is_whole() -> TestResult {
    let session = TempDir::new("assemble-stale-whole")?;
    let parts = session.path().join("parts/message");
    cache(&parts, 0, "第一段。\n")?;
    cache(&parts, 1, "第二段。\n")?;
    cache(&parts, 5, "后面的一批。\n")?;
    let mut stderr = Vec::new();

    let whole = assemble(
        &parts,
        2,
        Instant::now() + SIBLING_WAIT,
        session.path(),
        "message",
        now(),
        &mut stderr,
    )?;

    assert_eq!(whole.as_deref(), Some("第一段。\n第二段。\n"));
    assert_eq!(stderr, b"");
    Ok(())
}

/// Nothing was ever cached under this instance. That is a prefix with every
/// batch missing — the hole it reports — and not a directory that would not
/// read back, which is the other thing this returns and the caller treats
/// differently.
#[test]
fn assemble_reports_a_message_with_no_directory_at_all_as_a_hole() -> TestResult {
    let session = TempDir::new("assemble-absent")?;
    let parts = session.path().join("parts/message");
    let mut stderr = Vec::new();

    let whole = assemble(
        &parts,
        3,
        Instant::now(),
        session.path(),
        "message",
        now(),
        &mut stderr,
    )?;

    assert_eq!(whole, None);
    assert_eq!(
        String::from_utf8(stderr)?,
        "limae hook: 0/3 earlier batches arrived before the deadline; showing this one as it came\n"
    );
    Ok(())
}

/// A batch still being written is under its temporary name until the rename
/// puts it in place, so it is not one of the batches that are here yet. Reading
/// one would be reading half a paragraph.
#[test]
fn assemble_does_not_take_a_batch_that_is_still_being_written() -> TestResult {
    let session = TempDir::new("assemble-writing")?;
    let parts = session.path().join("parts/message");
    cache(&parts, 0, "第一段。\n")?;
    fs::write(
        parts.join(format!("000001.{}.writing", std::process::id())),
        "半",
    )?;
    let mut stderr = Vec::new();

    let whole = assemble(
        &parts,
        2,
        Instant::now(),
        session.path(),
        "message",
        now(),
        &mut stderr,
    )?;

    assert_eq!(whole, None);
    assert_eq!(
        String::from_utf8(stderr)?,
        "limae hook: 1/2 earlier batches arrived before the deadline; showing this one as it came\n"
    );
    Ok(())
}

// -- the two waits --------------------------------------------------------

/// Both values are spelled out here because both are load-bearing and neither
/// is derivable: the wait is two orders of magnitude past the time a sibling
/// takes to land, and the poll is short enough that the wait is not spent
/// sleeping through an arrival.
#[test]
fn the_wait_and_the_poll_are_the_values_the_host_behaviour_calls_for() {
    assert_eq!(SIBLING_WAIT, Duration::from_secs(2));
    assert_eq!(SIBLING_POLL, Duration::from_millis(20));
    assert!(SIBLING_POLL * 10 < SIBLING_WAIT);
}

// -- caching one batch ----------------------------------------------------

/// The same index twice with the same content is the host repeating itself;
/// with different content it is a different message, and the first one stays.
#[test]
fn a_batch_is_kept_once_and_a_different_one_under_its_index_is_a_conflict() -> TestResult {
    let session = TempDir::new("store")?;
    let parts = session.path().join("parts/message.turn");

    assert_eq!(store(&parts, 0, "甲\n")?, Stored::Kept);
    assert_eq!(store(&parts, 0, "甲\n")?, Stored::Repeated);
    assert_eq!(store(&parts, 0, "乙\n")?, Stored::Conflict);

    assert_eq!(fs::read_to_string(state::part(&parts, 0))?, "甲\n");
    Ok(())
}

// -- the prefix replay ----------------------------------------------------

/// A working directory with this `limae.toml` in it, for the arms that read
/// the configuration the way the hook does.
fn configured(name: &str, table: &str) -> Result<TempDir, Box<dyn Error>> {
    let cwd = TempDir::new(name)?;
    fs::write(cwd.path().join("limae.toml"), table)?;
    Ok(cwd)
}

fn fixed(prefix: &str, delta: &str, is_final: bool) -> Result<Outcome, Box<dyn Error>> {
    Ok(replay_with(
        &Pipeline::new()?,
        &ResolvedConfig::default(),
        prefix,
        delta,
        is_final,
    ))
}

/// The batch is fixed as the last lines of the whole message, and only those
/// lines come back — a batch of several lines comes back as several lines.
#[test]
fn a_batch_is_fixed_in_the_light_of_its_prefix_and_only_its_own_lines_come_back() -> TestResult {
    assert_eq!(
        fixed("甲,乙\n", "丙,丁\n戊,己\n", false)?,
        Outcome::Fixed("丙，丁\n戊，己\n".to_owned())
    );
    assert_eq!(
        fixed("", "丙,丁\n", false)?,
        Outcome::Fixed("丙，丁\n".to_owned())
    );
    // Already what the fixes would make it: nothing to say.
    assert_eq!(fixed("甲,乙\n", "丙，丁\n", false)?, Outcome::Unchanged);
    // An empty batch decides nothing.
    assert_eq!(fixed("甲,乙\n", "", true)?, Outcome::Unchanged);
    Ok(())
}

/// The prefix is the state: inside a fence the batch is code and is left alone,
/// and the same batch with no prefix is prose and is not. The second arm is
/// what proves the first one is the prefix's doing.
#[test]
fn the_prefix_is_what_keeps_code_in_a_fence_from_being_fixed_as_prose() -> TestResult {
    let code = "result.status.success()\n";
    assert_eq!(fixed("```rust\n", code, false)?, Outcome::Unchanged);
    assert_eq!(
        fixed("", code, false)?,
        Outcome::Fixed("result.status.success ()\n".to_owned())
    );
    Ok(())
}

/// The ending is the batch's own: a middle batch keeps its line feed, a final
/// batch without one gets none, and a carriage return is a byte like any other.
/// These go through `Pipeline::fix` and not the CLI's file I/O, which is what
/// folds CRLF to LF and would hide the last arm.
#[test]
fn the_ending_of_the_answer_is_the_ending_of_the_batch_byte_for_byte() -> TestResult {
    assert_eq!(
        fixed("甲,乙\n", "丙,丁\n", false)?,
        Outcome::Fixed("丙，丁\n".to_owned())
    );
    assert_eq!(
        fixed("甲,乙\n", "丙,丁", true)?,
        Outcome::Fixed("丙，丁".to_owned())
    );
    assert_eq!(
        fixed("甲,乙\r\n", "丙,丁\r\n", false)?,
        Outcome::Fixed("丙，丁\r\n".to_owned())
    );
    Ok(())
}

/// A batch boundary that is not a line boundary cannot be sliced at one. The
/// arm with the line feed put back is what says the refusal is about the
/// boundary and not about the text.
#[test]
fn a_batch_boundary_inside_a_line_is_partial_and_left_alone() -> TestResult {
    let partial = Outcome::Declined(Step::Siblings, Kind::Partial);
    // The prefix ends mid-line, so this batch starts mid-line.
    assert_eq!(fixed("甲,乙", "丙,丁\n", false)?, partial);
    // This middle batch ends mid-line.
    assert_eq!(fixed("甲,乙\n", "丙,丁", false)?, partial);
    // The same batch as the final one: a final batch ends where the message
    // does, and that is a line boundary.
    assert_eq!(
        fixed("甲,乙\n", "丙,丁", true)?,
        Outcome::Fixed("丙，丁".to_owned())
    );
    Ok(())
}

/// The shape of `spec/fixtures/span-across-line-break`, cut where the span
/// crosses the line: the opening batch is not fixed, because the next batch
/// could turn it into code; the closing batch is, outside the span. The control
/// arm puts both lines in one batch, and the span is then decided.
#[test]
fn a_batch_that_may_still_be_inside_a_code_span_waits_for_the_next() -> TestResult {
    let opening = "`你好,世界\n";
    let closing = "函数(x)` 后文(y)";
    assert_eq!(
        fixed("", opening, false)?,
        Outcome::Declined(Step::Siblings, Kind::Unclosed)
    );
    assert_eq!(
        fixed(opening, closing, true)?,
        Outcome::Fixed("函数(x)` 后文 (y)".to_owned())
    );
    assert_eq!(
        fixed("", &format!("{opening}{closing}"), true)?,
        Outcome::Fixed("`你好,世界\n函数(x)` 后文 (y)".to_owned())
    );
    // The final batch has nothing after it, so an unpaired run in it is
    // ordinary text and the batch is fixed.
    assert_eq!(
        fixed("", "未闭合`的反引号(x)", true)?,
        Outcome::Fixed("未闭合`的反引号 (x)".to_owned())
    );
    assert_eq!(
        fixed("", "未闭合`的反引号(x)\n", false)?,
        Outcome::Declined(Step::Siblings, Kind::Unclosed)
    );
    // A closed span, and a fence, are decided already.
    assert_eq!(
        fixed("", "`函数(x)` 后文(y)\n", false)?,
        Outcome::Fixed("`函数(x)` 后文 (y)\n".to_owned())
    );
    assert_eq!(fixed("```\n", "函数(x)`\n", false)?, Outcome::Unchanged);
    Ok(())
}

/// The configuration is the repository's, found from `cwd` the way `limae
/// --fix` finds it. One that will not read is the user's to fix and is named
/// as such, not as a crash; a directive in the reply naming a rule that does
/// not exist is the same person's problem and gets the same name.
#[test]
fn the_configuration_is_read_from_cwd_and_one_that_will_not_read_is_named() -> TestResult {
    let disabled = configured("replay-disabled", "disable = [\"zh-typography-1\"]\n")?;
    assert_eq!(
        replay("", "丙,丁\n", false, disabled.path()),
        Outcome::Unchanged
    );
    let plain = TempDir::new("replay-plain")?;
    assert_eq!(
        replay("", "丙,丁\n", false, plain.path()),
        Outcome::Fixed("丙，丁\n".to_owned())
    );
    let broken = configured("replay-broken", "disable = [\n")?;
    assert_eq!(
        replay("", "丙,丁\n", false, broken.path()),
        Outcome::Declined(Step::Fix, Kind::Misconfigured)
    );
    assert_eq!(
        replay(
            "<!-- limae-disable no-such-rule -->\n",
            "丙,丁\n",
            false,
            plain.path()
        ),
        Outcome::Declined(Step::Fix, Kind::Misconfigured)
    );
    Ok(())
}

// -- every golden fixture, batch by batch ---------------------------------

/// Cut one text into batches the way the host does: whole lines, `sizes` of
/// them to a batch (cycling), and last whatever follows the final line feed,
/// which is the final batch and may be empty.
fn batched(text: &str, sizes: &[usize]) -> Vec<(String, bool)> {
    let mut batches = Vec::new();
    let mut lines = text.split_inclusive('\n').peekable();
    let mut size = sizes.iter().cycle();
    while lines.peek().is_some_and(|line| line.ends_with('\n')) {
        let mut batch = String::new();
        for _ in 0..*size.next().unwrap_or(&1) {
            match lines.next_if(|line| line.ends_with('\n')) {
                Some(line) => batch.push_str(line),
                None => break,
            }
        }
        batches.push((batch, false));
    }
    batches.push((lines.next().unwrap_or_default().to_owned(), true));
    batches
}

/// Feed one text through the replay batch by batch, with the real prefix or
/// with none, and put the answers back together.
///
/// Returns the reassembled text and, for each batch, what the replay decided:
/// `None` for an answer or no answer, `Some(kind)` for a refusal.
fn swept(
    pipeline: &Pipeline,
    config: &ResolvedConfig,
    text: &str,
    sizes: &[usize],
    prefixed: bool,
) -> (String, Vec<(String, Option<Kind>)>) {
    let mut prefix = String::new();
    let mut shown = String::new();
    let mut decided = Vec::new();
    for (delta, is_final) in batched(text, sizes) {
        let given = if prefixed { prefix.as_str() } else { "" };
        let (answer, declined) = match replay_with(pipeline, config, given, &delta, is_final) {
            Outcome::Fixed(answer) => (answer, None),
            Outcome::Unchanged => (delta.clone(), None),
            Outcome::Declined(_, kind) => (delta.clone(), Some(kind)),
        };
        shown.push_str(&answer);
        decided.push((answer, declined));
        prefix.push_str(&delta);
    }
    (shown, decided)
}

/// Cut a fixed whole into the pieces its batches should have come back as:
/// the same line feeds, so the same line counts.
fn expected(whole: &str, batches: &[(String, Option<Kind>)]) -> Vec<String> {
    let mut rest = whole;
    let mut pieces = Vec::new();
    for (answer, _) in batches {
        let feeds = answer.matches('\n').count();
        let end = if feeds == 0 {
            rest.len()
        } else {
            rest.match_indices('\n')
                .nth(feeds - 1)
                .map_or(rest.len(), |(at, _)| at + 1)
        };
        pieces.push(rest[..end].to_owned());
        rest = &rest[end..];
    }
    pieces
}

/// ADR-0016's first acceptance criterion: batch by batch equals the whole, for
/// every golden fixture, cut one line to a batch and cut several.
///
/// Every batch the replay answers comes back as the matching lines of the
/// whole fix, byte for byte; the one thing allowed to differ is a batch
/// declined as unclosed, which goes up as it came. Three lists are written
/// out. The fixtures with such a batch are exactly the ones a backtick run is
/// still open in at the end of a batch: a replay that declined nothing would
/// answer those batches wrongly and go red on the byte-for-byte assertion, and
/// one that declined every batch with a backtick in it would lengthen the
/// list. The fixtures whose reassembled text differs from the whole are the
/// six where a declined middle batch also holds prose the whole would fix
/// beside the open run — the price ADR-0016 section 二 accepts, and no wider:
/// `span-across-line-break` is declined too and does not differ, because its
/// declined batch is all span.
///
/// The control arm runs the same sweep with the prefix withheld. The fenced
/// and directive fixtures then come apart from the whole, which is what says
/// the prefix is load-bearing.
#[test]
fn every_golden_fixture_reassembles_to_its_whole_fix_batch_by_batch() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/fixtures");
    let mut inputs = fs::read_dir(&root)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    inputs.retain(|path| path.extension().is_some_and(|extension| extension == "in"));
    inputs.sort();
    assert!(!inputs.is_empty(), "no golden fixtures discovered");
    let pipeline = Pipeline::new()?;
    let temporary = TempDir::new("fixtures")?;

    let mut with_unclosed = Vec::new();
    let mut apart = Vec::new();
    let mut apart_without_prefix = Vec::new();
    for input in &inputs {
        let case = input
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or("case name")?
            .to_owned();
        let conf = input.with_extension("conf");
        let config = if conf.try_exists()? {
            fs::copy(conf, temporary.path().join("limae.toml"))?;
            resolve(temporary.path(), CliOverrides::default())?
        } else {
            ResolvedConfig::default()
        };
        let text = fs::read_to_string(input)?;
        let whole = pipeline.fix(&text, &config)?;
        let (mut declined, mut differs, mut differs_alone) = (false, false, false);
        for sizes in [&[1][..], &[2, 3][..], &[3, 1, 2][..]] {
            let (shown, batches) = swept(&pipeline, &config, &text, sizes, true);
            for (index, ((answer, kind), wanted)) in
                batches.iter().zip(expected(&whole, &batches)).enumerate()
            {
                match kind {
                    None => assert_eq!(answer, &wanted, "{case} cut {sizes:?} batch {index}"),
                    Some(Kind::Unclosed) => declined = true,
                    Some(other) => panic!("{case} cut {sizes:?} batch {index}: {other:?}"),
                }
            }
            differs |= shown != whole;
            let (alone, _) = swept(&pipeline, &config, &text, sizes, false);
            differs_alone |= alone != whole;
        }
        if declined {
            with_unclosed.push(case.clone());
        }
        if differs {
            apart.push(case.clone());
        }
        if differs_alone {
            apart_without_prefix.push(case);
        }
    }

    assert_eq!(
        with_unclosed,
        [
            "inline-code-spans",
            "span-across-blockquote-lines",
            "span-across-line-break",
            "span-closes-at-line-start",
            "span-continues-in-list-item",
            "span-not-across-blank-line",
            "span-not-across-heading",
            "span-not-across-list-items",
            "span-opens-at-line-end",
            "zh-typography-7-code-spacing",
        ]
    );
    assert_eq!(
        apart,
        [
            "inline-code-spans",
            "span-not-across-blank-line",
            "span-not-across-heading",
            "span-not-across-list-items",
            "span-opens-at-line-end",
            "zh-typography-7-code-spacing",
        ]
    );
    for needs_prefix in [
        "fenced-code",
        "inline-disable-range",
        "inline-disable-next-line",
    ] {
        assert!(
            apart_without_prefix.iter().any(|case| case == needs_prefix),
            "{needs_prefix} came out the same with no prefix"
        );
        assert!(
            !apart.iter().any(|case| case == needs_prefix),
            "{needs_prefix} came apart under the replay"
        );
    }
    Ok(())
}

// -- the limits -----------------------------------------------------------

/// The values are spelled out because they are what a message is held to, and
/// the wait is the sibling wait and not a second number.
#[test]
fn the_default_limits_are_the_values_the_cost_analysis_calls_for() {
    assert_eq!(
        Limits::DEFAULT,
        Limits {
            bytes: 256 * 1024,
            batches: 1000,
            wait: SIBLING_WAIT,
        }
    );
}

/// Two batches for the same index at once, over and over: exactly one is
/// kept and the other is a conflict, every time. A `store` that read first
/// and wrote after would have both find nothing and both keep, and the pair
/// would come back `(Kept, Kept)` some of the time (2026-09-11, review of
/// PR #180: 59 of 500 pairs of real processes).
#[test]
fn two_batches_for_one_index_at_once_are_one_kept_and_one_conflict() -> TestResult {
    let session = TempDir::new("store-race")?;
    for round in 0..200 {
        let parts = session.path().join(format!("parts/message-{round}"));
        let (first, second) = (parts.clone(), parts.clone());
        let a = std::thread::spawn(move || store(&first, 0, "甲\n"));
        let b = std::thread::spawn(move || store(&second, 0, "乙\n"));
        let mut outcomes = [
            a.join().map_err(|_| "thread a panicked")??,
            b.join().map_err(|_| "thread b panicked")??,
        ];
        outcomes.sort_by_key(|outcome| *outcome == Stored::Conflict);
        assert_eq!(outcomes, [Stored::Kept, Stored::Conflict], "round {round}");
        let kept = fs::read_to_string(state::part(&parts, 0))?;
        assert!(kept == "甲\n" || kept == "乙\n", "round {round}: {kept:?}");
    }
    Ok(())
}
