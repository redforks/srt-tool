//! End-to-end tests that drive the built `srt-tool` binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_srt-tool");

const SAMPLE: &str = "1\n\
00:00:01,000 --> 00:00:02,500\n\
Hello there\n\
\n\
2\n\
00:00:03,000 --> 00:00:04,000\n\
Second line\n\
second cue line 2\n\
\n";

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A minimal script whose `[Events]` `Format:` line has the usual field order.
const ASS_SAMPLE: &str = "[Script Info]\n\
Title: sample\n\
\n\
[Events]\n\
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
Dialogue: 0,0:00:01.00,0:00:02.50,Default,,0,0,0,,Hello\n";

/// [`ASS_SAMPLE`] after `+1s`.
const ASS_SAMPLE_ONE_SECOND_LATER: &str = "[Script Info]\n\
Title: sample\n\
\n\
[Events]\n\
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
Dialogue: 0,0:00:02.00,0:00:03.50,Default,,0,0,0,,Hello\n";

/// A script with everything a preserving rewrite must not disturb: a byte-order
/// mark, CRLF line endings, a `;` comment, styles, a `Comment:` event, override
/// tags, trailing whitespace on a cue, and no final newline.
const ASS_RICH: &str = "\u{feff}[Script Info]\r\n\
; a comment the tool must keep\r\n\
Title: Sample\r\n\
ScriptType: v4.00+\r\n\
\r\n\
[V4+ Styles]\r\n\
Format: Name, Fontname, Fontsize, PrimaryColour\r\n\
Style: Default,Arial,20,&H00FFFFFF\r\n\
\r\n\
[Events]\r\n\
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\r\n\
Comment: 0,0:00:09.00,0:00:10.00,Default,,0,0,0,,Static note\r\n\
Dialogue: 0,0:00:01.00,0:00:02.50,Default,,0,0,0,,{\\an8}Top line   \r\n\
Dialogue: 1,0:00:03.00,0:00:04.00,Default,,0,0,0,,{\\k20}Second";

/// [`ASS_RICH`] after `+1s`.
const ASS_RICH_ONE_SECOND_LATER: &str = "\u{feff}[Script Info]\r\n\
; a comment the tool must keep\r\n\
Title: Sample\r\n\
ScriptType: v4.00+\r\n\
\r\n\
[V4+ Styles]\r\n\
Format: Name, Fontname, Fontsize, PrimaryColour\r\n\
Style: Default,Arial,20,&H00FFFFFF\r\n\
\r\n\
[Events]\r\n\
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\r\n\
Comment: 0,0:00:09.00,0:00:10.00,Default,,0,0,0,,Static note\r\n\
Dialogue: 0,0:00:02.00,0:00:03.50,Default,,0,0,0,,{\\an8}Top line   \r\n\
Dialogue: 1,0:00:04.00,0:00:05.00,Default,,0,0,0,,{\\k20}Second";

/// A temporary directory, removed when it goes out of scope.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("srt-tool-test-{}-{serial}", std::process::id()));
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    /// Write `contents` to `name` inside the temporary directory, return the path.
    fn write(&self, name: &str, contents: &str) -> PathBuf {
        self.write_bytes(name, contents.as_bytes())
    }

    /// Write raw `bytes` to `name` inside the temporary directory, return the path.
    fn write_bytes(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path.join(name);
        fs::write(&path, bytes).expect("write temp file");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().expect("run srt-tool")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("process exited via a code")
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("temp path is UTF-8")
}

#[test]
fn shift_moves_cues_later_on_stdout() {
    let dir = TempDir::new();
    let file = dir.write("movie.srt", SAMPLE);

    let output = run(&["shift", path_str(&file), "+1500ms"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "1\n\
         00:00:02,500 --> 00:00:04,000\n\
         Hello there\n\
         \n\
         2\n\
         00:00:04,500 --> 00:00:05,500\n\
         Second line\n\
         second cue line 2\n\
         \n"
    );
    assert_eq!(stderr(&output), "");
}

#[test]
fn shift_accepts_seconds_and_fractions_and_uppercase_units() {
    let dir = TempDir::new();
    let file = dir.write("movie.srt", SAMPLE);

    for (offset, expected_first_start) in [
        ("+2s", "00:00:03,000"),
        ("+1.5s", "00:00:02,500"),
        ("+1500MS", "00:00:02,500"),
        ("+0.25s", "00:00:01,250"),
    ] {
        let output = run(&["shift", path_str(&file), offset]);
        assert_eq!(
            code(&output),
            0,
            "offset {offset} failed: {}",
            stderr(&output)
        );
        assert!(
            stdout(&output).starts_with(&format!("1\n{expected_first_start} --> ")),
            "offset {offset} produced: {}",
            stdout(&output)
        );
    }
}

#[test]
fn shift_moves_cues_earlier() {
    let dir = TempDir::new();
    let file = dir.write("movie.srt", SAMPLE);

    let output = run(&["shift", path_str(&file), "-1s"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("00:00:00,000 --> 00:00:01,500"), "{out}");
    assert!(out.contains("00:00:02,000 --> 00:00:03,000"), "{out}");
    assert_eq!(stderr(&output), "");
}

#[test]
fn clamping_reports_one_aggregate_warning() {
    let dir = TempDir::new();
    let file = dir.write("movie.srt", "1\n00:00:00,500 --> 00:00:01,000\nEarly\n\n");

    let output = run(&["shift", path_str(&file), "-2s"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "1\n00:00:00,000 --> 00:00:00,000\nEarly\n\n"
    );
    assert_eq!(
        stderr(&output),
        "warning: 2 cue times clamped to 00:00:00,000\n"
    );
}

#[test]
fn clamping_uses_singular_wording_for_one_time() {
    let dir = TempDir::new();
    let file = dir.write("movie.srt", "1\n00:00:01,000 --> 00:00:09,000\nLate\n\n");

    let output = run(&["shift", path_str(&file), "-2s"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(
        stderr(&output),
        "warning: 1 cue time clamped to 00:00:00,000\n"
    );
}

#[test]
fn offset_without_sign_is_a_usage_error() {
    let dir = TempDir::new();
    let file = dir.write("movie.srt", SAMPLE);

    let output = run(&["shift", path_str(&file), "500ms"]);

    assert_eq!(code(&output), 2, "stderr: {}", stderr(&output));
    assert!(stdout(&output).is_empty());
    assert!(stderr(&output).contains('+'), "stderr: {}", stderr(&output));
}

#[test]
fn offset_without_unit_is_a_usage_error() {
    let dir = TempDir::new();
    let file = dir.write("movie.srt", SAMPLE);

    for bad in [
        "+500", "-2", "+5x", "+.5s", "+1.s", "+", "+s", "+500 ms", " +500ms",
    ] {
        let output = run(&["shift", path_str(&file), bad]);
        assert_eq!(code(&output), 2, "offset {bad:?} was accepted");
        assert_eq!(stdout(&output), "", "offset {bad:?} wrote to stdout");
    }
}

#[test]
fn missing_file_argument_is_a_usage_error() {
    let output = run(&["shift"]);

    assert_eq!(code(&output), 2);
}

#[test]
fn unknown_subcommand_is_a_usage_error() {
    let output = run(&["scale", "movie.srt"]);

    assert_eq!(code(&output), 2);
}

#[test]
fn help_and_version_succeed() {
    let help = run(&["--help"]);
    assert_eq!(code(&help), 0);
    assert!(stdout(&help).contains("shift"));

    let version = run(&["--version"]);
    assert_eq!(code(&version), 0);
    assert!(stdout(&version).contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn unreadable_file_exits_with_code_3() {
    let dir = TempDir::new();
    let missing = dir.path.join("nope.srt");

    let output = run(&["shift", path_str(&missing), "+1s"]);

    assert_eq!(code(&output), 3, "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("cannot read"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn malformed_timing_line_exits_with_code_3() {
    let dir = TempDir::new();
    let file = dir.write("broken.srt", "1\nnot a timestamp\nHello\n\n");

    let output = run(&["shift", path_str(&file), "+1s"]);

    assert_eq!(code(&output), 3, "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("cannot parse"),
        "stderr: {}",
        stderr(&output)
    );
    assert_eq!(stdout(&output), "");
}

#[test]
fn non_numeric_cue_index_exits_with_code_3() {
    let dir = TempDir::new();
    let file = dir.write(
        "broken.srt",
        "one\n00:00:01,000 --> 00:00:02,000\nHello\n\n",
    );

    let output = run(&["shift", path_str(&file), "+1s"]);

    assert_eq!(code(&output), 3, "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("line 1"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn output_is_canonical() {
    let dir = TempDir::new();
    // BOM, CRLF, odd indices, missing final newline, extra blank lines.
    let file = dir.write(
        "messy.srt",
        "\u{feff}7\r\n00:00:01,000 --> 00:00:02,500\r\nHello\r\n\r\n\r\n9\r\n00:00:03,000 --> 00:00:04,000\r\nWorld",
    );

    let output = run(&["shift", path_str(&file), "+0s"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "1\n00:00:01,000 --> 00:00:02,500\nHello\n\n2\n00:00:03,000 --> 00:00:04,000\nWorld\n\n"
    );
}

#[test]
fn text_lines_are_preserved_verbatim() {
    let dir = TempDir::new();
    let file = dir.write(
        "styled.srt",
        "1\n00:00:01,000 --> 00:00:02,000\n<i>Hello</i>\n{\\an8}Top line\n\n",
    );

    let output = run(&["shift", path_str(&file), "+1s"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("<i>Hello</i>\n{\\an8}Top line\n"));
}

#[test]
fn empty_input_produces_empty_output() {
    let dir = TempDir::new();
    let file = dir.write("empty.srt", "");

    let output = run(&["shift", path_str(&file), "+1s"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "");
}

#[test]
fn in_place_rewrites_the_file_and_keeps_a_backup() {
    let dir = TempDir::new();
    let file = dir.write("movie.srt", SAMPLE);
    let backup = dir.path.join("movie.srt.bak");

    let output = run(&["shift", path_str(&file), "+1s", "--in-place"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "");
    let rewritten = fs::read_to_string(&file).expect("read rewritten file");
    assert!(
        rewritten.contains("00:00:02,000 --> 00:00:03,500"),
        "{rewritten}"
    );
    assert_eq!(
        fs::read_to_string(&backup).expect("read backup"),
        SAMPLE,
        "backup must hold the original bytes"
    );
    assert!(
        !dir.path.join("movie.srt.srt-tool.tmp").exists(),
        "temporary file must not be left behind"
    );
}

#[test]
fn in_place_overwrites_an_existing_backup() {
    let dir = TempDir::new();
    let file = dir.write("movie.srt", SAMPLE);
    let backup = dir.write("movie.srt.bak", "stale backup");

    let output = run(&["shift", path_str(&file), "+1s", "--in-place"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(fs::read_to_string(&backup).expect("read backup"), SAMPLE);
}

#[test]
fn in_place_flag_may_precede_positionals() {
    let dir = TempDir::new();
    let file = dir.write("movie.srt", SAMPLE);

    let output = run(&["shift", "--in-place", path_str(&file), "+1s"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "");
}

#[test]
fn in_place_on_missing_file_exits_with_code_3() {
    let dir = TempDir::new();
    let missing = dir.path.join("nope.srt");

    let output = run(&["shift", path_str(&missing), "+1s", "--in-place"]);

    assert_eq!(code(&output), 3, "stderr: {}", stderr(&output));
}

#[test]
fn unsupported_extension_exits_with_code_3_and_writes_nothing() {
    let dir = TempDir::new();
    let file = dir.write("movie.txt", SAMPLE);

    let output = run(&["shift", path_str(&file), "+1s"]);

    assert_eq!(code(&output), 3, "stderr: {}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "",
        "an unsupported file must write nothing"
    );
    assert_eq!(
        stderr(&output),
        "error: unsupported file extension \".txt\": expected .srt, .ass, or .ssa\n"
    );
    assert_eq!(
        fs::read_to_string(&file).expect("read untouched file"),
        SAMPLE,
        "an unsupported file must not be rewritten"
    );
}

#[test]
fn uppercase_extension_reads_the_same_as_lowercase() {
    let dir = TempDir::new();
    let lower = dir.write("movie.srt", SAMPLE);
    let upper = dir.write("movie.SRT", SAMPLE);

    let lower_output = run(&["shift", path_str(&lower), "+1s"]);
    let upper_output = run(&["shift", path_str(&upper), "+1s"]);

    assert_eq!(code(&upper_output), 0, "stderr: {}", stderr(&upper_output));
    assert_eq!(stdout(&upper_output), stdout(&lower_output));
    assert_eq!(stderr(&upper_output), "");
    assert!(
        stdout(&upper_output).contains("00:00:02,000 --> 00:00:03,500"),
        "{}",
        stdout(&upper_output)
    );
}

#[test]
fn missing_extension_exits_with_code_3_and_names_the_accepted_ones() {
    let dir = TempDir::new();
    let file = dir.write("movie", SAMPLE);

    let output = run(&["shift", path_str(&file), "+1s"]);

    assert_eq!(code(&output), 3, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        "error: missing file extension: expected .srt, .ass, or .ssa\n"
    );
}

#[test]
fn ass_and_ssa_extensions_select_the_ass_format() {
    let dir = TempDir::new();

    for name in ["movie.ass", "movie.ssa", "movie.ASS"] {
        let file = dir.write(name, ASS_SAMPLE);

        let output = run(&["shift", path_str(&file), "+1s"]);

        assert_eq!(code(&output), 0, "{name}: stderr: {}", stderr(&output));
        assert_eq!(stdout(&output), ASS_SAMPLE_ONE_SECOND_LATER, "{name}");
        assert_eq!(stderr(&output), "", "{name}");
    }
}

#[test]
fn ass_zero_offset_is_byte_identical() {
    let dir = TempDir::new();
    let file = dir.write("movie.ass", ASS_RICH);

    let output = run(&["shift", path_str(&file), "+0ms"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(
        output.stdout,
        ASS_RICH.as_bytes(),
        "a zero shift must return the input bytes"
    );
    assert_eq!(stderr(&output), "");
}

#[test]
fn ass_shift_moves_only_the_timings() {
    let dir = TempDir::new();
    let file = dir.write("movie.ass", ASS_RICH);

    let output = run(&["shift", path_str(&file), "+1s"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(
        stdout(&output),
        ASS_RICH_ONE_SECOND_LATER,
        "only Start and End may change"
    );
    assert_eq!(stderr(&output), "");
}

#[test]
fn ass_comment_lines_styles_and_override_tags_do_not_move() {
    let dir = TempDir::new();
    let file = dir.write("movie.ass", ASS_RICH);

    let output = run(&["shift", path_str(&file), "-2s"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(
        out.contains("Comment: 0,0:00:09.00,0:00:10.00,Default,,0,0,0,,Static note"),
        "a Comment: line is not a cue: {out}"
    );
    assert!(
        out.contains("Style: Default,Arial,20,&H00FFFFFF"),
        "a style must survive: {out}"
    );
    assert!(
        out.contains("{\\an8}Top line"),
        "override tags must survive: {out}"
    );
    assert!(out.contains("{\\k20}Second"), "{out}");
    assert!(
        out.contains("Dialogue: 0,0:00:00.00,0:00:00.50,Default,,0,0,0,,{\\an8}Top line"),
        "timings move: {out}"
    );
}

#[test]
fn ass_clamping_reports_the_same_warning_and_exit_code() {
    let dir = TempDir::new();
    let file = dir.write(
        "early.ass",
        "[Events]\nFormat: Layer, Start, End, Style, Text\n\
         Dialogue: 0,0:00:00.50,0:00:01.00,Default,Early\n",
    );

    let output = run(&["shift", path_str(&file), "-2s"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("Dialogue: 0,0:00:00.00,0:00:00.00,Default,Early"),
        "{}",
        stdout(&output)
    );
    assert_eq!(
        stderr(&output),
        "warning: 2 cue times clamped to 00:00:00,000\n"
    );
}

#[test]
fn ass_missing_format_line_exits_with_code_3_and_names_the_line() {
    let dir = TempDir::new();
    let file = dir.write(
        "broken.ass",
        "[Script Info]\nTitle: Broken\n\n[Events]\n\
         Dialogue: 0,0:00:01.00,0:00:02.00,Default,Hello\n",
    );

    let output = run(&["shift", path_str(&file), "+1s"]);

    assert_eq!(code(&output), 3, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "");
    let message = stderr(&output);
    assert!(message.contains("cannot parse"), "{message}");
    assert!(message.contains("line 5"), "{message}");
    assert!(message.contains("Format: line"), "{message}");
}

#[test]
fn ass_event_with_too_few_fields_exits_with_code_3_and_names_the_line() {
    let dir = TempDir::new();
    let file = dir.write(
        "broken.ass",
        "[Events]\nFormat: Layer, Start, End, Style, Text\n\
         Dialogue: 0,0:00:01.00,Default\n",
    );

    let output = run(&["shift", path_str(&file), "+1s"]);

    assert_eq!(code(&output), 3, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        format!(
            "error: cannot parse {}: line 3: expected 5 fields in the event line, found 3\n",
            file.display()
        )
    );
}

#[test]
fn ass_in_place_keeps_the_bytes_and_the_backup() {
    let dir = TempDir::new();
    let file = dir.write("movie.ass", ASS_RICH);
    let backup = dir.path.join("movie.ass.bak");

    let output = run(&["shift", path_str(&file), "+1s", "--in-place"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "");
    let rewritten = fs::read(&file).expect("read rewritten file");
    assert_eq!(rewritten, ASS_RICH_ONE_SECOND_LATER.as_bytes());
    assert_eq!(
        fs::read(&backup).expect("read backup"),
        ASS_RICH.as_bytes(),
        "the backup holds the original bytes, BOM and CRLF included"
    );
}

#[test]
fn ass_file_without_an_events_section_is_unchanged() {
    let dir = TempDir::new();
    let text = "[Script Info]\nTitle: No events\n";
    let file = dir.write("empty.ass", text);

    let output = run(&["shift", path_str(&file), "+5s"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), text);
}

#[test]
fn non_utf8_input_exits_with_code_3_and_is_not_transcoded() {
    let dir = TempDir::new();
    // A lone 0xE9 is Latin-1, not UTF-8, and must not be guessed at.
    let bytes: &[u8] = b"[Events]\nFormat: Layer, Start, End, Style, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,\xe9\n";
    let file = dir.write_bytes("latin1.ass", bytes);

    let output = run(&["shift", path_str(&file), "+1s", "--in-place"]);

    assert_eq!(code(&output), 3, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "");
    let message = stderr(&output);
    assert!(message.contains("not valid UTF-8"), "{message}");
    assert!(!dir.path.join("latin1.ass.bak").exists());
    assert_eq!(
        fs::read(&file).expect("read untouched file"),
        bytes,
        "a file that cannot be decoded must not be rewritten"
    );
}

#[test]
fn in_place_keeps_the_input_extension_in_the_backup_name() {
    let dir = TempDir::new();
    let file = dir.write("movie.SRT", SAMPLE);
    let backup = dir.path.join("movie.SRT.bak");

    let output = run(&["shift", path_str(&file), "+1s", "--in-place"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "");
    assert_eq!(fs::read_to_string(&backup).expect("read backup"), SAMPLE);
    let rewritten = fs::read_to_string(&file).expect("read rewritten file");
    assert!(
        rewritten.contains("00:00:02,000 --> 00:00:03,500"),
        "{rewritten}"
    );
}

#[test]
fn in_place_on_an_unsupported_extension_writes_no_backup() {
    let dir = TempDir::new();
    let file = dir.write("movie.txt", SAMPLE);

    let output = run(&["shift", path_str(&file), "+1s", "--in-place"]);

    assert_eq!(code(&output), 3, "stderr: {}", stderr(&output));
    assert!(!dir.path.join("movie.txt.bak").exists());
    assert!(!dir.path.join("movie.txt.srt-tool.tmp").exists());
    assert_eq!(fs::read_to_string(&file).expect("read file"), SAMPLE);
}

#[test]
fn ass_rounding_reports_how_many_timestamps_were_rounded() {
    let dir = TempDir::new();
    let file = dir.write("movie.ass", ASS_SAMPLE);

    // 505ms cannot be written: every timestamp gains 510ms of written value.
    let output = run(&["shift", path_str(&file), "+505ms"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("Dialogue: 0,0:00:01.51,0:00:03.01,Default,,0,0,0,,Hello"),
        "{}",
        stdout(&output)
    );
    assert_eq!(stderr(&output), "warning: 2 timestamps rounded to 10ms\n");
}

#[test]
fn ass_an_offset_of_whole_centiseconds_prints_no_warning() {
    let dir = TempDir::new();
    let file = dir.write("movie.ass", ASS_SAMPLE);

    let output = run(&["shift", path_str(&file), "+500ms"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("Dialogue: 0,0:00:01.50,0:00:03.00,Default,,0,0,0,,Hello"),
        "{}",
        stdout(&output)
    );
    assert_eq!(stderr(&output), "", "10ms needs no rounding");
}

#[test]
fn ass_rounding_and_clamping_print_one_warning_each_clamping_first() {
    let dir = TempDir::new();
    let file = dir.write(
        "early.ass",
        "[Events]\nFormat: Layer, Start, End, Style, Text\n\
         Dialogue: 0,0:00:00.50,0:00:05.00,Default,Early\n",
    );

    let output = run(&["shift", path_str(&file), "-1995ms"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("Dialogue: 0,0:00:00.00,0:00:03.01,Default,Early"),
        "{}",
        stdout(&output)
    );
    assert_eq!(
        stderr(&output),
        "warning: 1 cue time clamped to 00:00:00,000\n\
         warning: 1 timestamp rounded to 10ms\n"
    );
}

#[test]
fn srt_timestamps_stay_exact_when_ass_would_round() {
    let dir = TempDir::new();
    let file = dir.write("movie.srt", SAMPLE);

    let output = run(&["shift", path_str(&file), "+505ms"]);

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("00:00:01,505 --> 00:00:03,005"),
        "{}",
        stdout(&output)
    );
    assert_eq!(stderr(&output), "", "SubRip has no centisecond field");
}
