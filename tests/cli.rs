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
        let path = self.path.join(name);
        fs::write(&path, contents).expect("write temp file");
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
    assert_eq!(stdout(&output), "", "an unsupported file must write nothing");
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
