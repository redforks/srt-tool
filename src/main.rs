//! Command line interface for `srt-tool`.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use srt_tool::{Subtitle, TimeOffset};

/// Exit code for a file that cannot be read, parsed, or written.
const EXIT_FILE_ERROR: u8 = 3;

/// Suffix of the copy kept by `--in-place`.
const BACKUP_SUFFIX: &str = ".bak";

/// Suffix of the temporary file written before an in-place replace.
const TEMP_SUFFIX: &str = ".srt-tool.tmp";

#[derive(Debug, Parser)]
#[command(
    name = "srt-tool",
    version,
    about = "A small command line editor for subtitle files"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Move every cue by a signed offset
    Shift {
        /// Subtitle file to read
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Signed offset built from '+' or '-', a number, and 'ms' or 's'
        // A negative offset like `-2s` starts with a hyphen, so clap must not
        // mistake it for a flag.
        #[arg(value_name = "OFFSET", value_parser = parse_offset, allow_hyphen_values = true)]
        offset: TimeOffset,

        /// Rewrite FILE in place, keeping the original as FILE.bak
        #[arg(long)]
        in_place: bool,
    },
}

fn parse_offset(text: &str) -> Result<TimeOffset, srt_tool::OffsetError> {
    text.parse()
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(EXIT_FILE_ERROR)
        }
    }
}

fn run(cli: &Cli) -> Result<(), String> {
    match &cli.command {
        Command::Shift {
            file,
            offset,
            in_place,
        } => shift(file, *offset, *in_place),
    }
}

fn shift(file: &Path, offset: TimeOffset, in_place: bool) -> Result<(), String> {
    let original =
        fs::read(file).map_err(|error| format!("cannot read {}: {error}", file.display()))?;
    let text = std::str::from_utf8(&original).map_err(|_| {
        format!(
            "cannot read {}: the file is not valid UTF-8",
            file.display()
        )
    })?;

    let mut subtitle = Subtitle::parse(text)
        .map_err(|error| format!("cannot parse {}: {error}", file.display()))?;
    let report = subtitle.shift(offset);
    let output = subtitle.to_canonical_string();

    if in_place {
        replace_in_place(file, &original, &output)?;
    } else {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        handle
            .write_all(output.as_bytes())
            .and_then(|()| handle.flush())
            .map_err(|error| format!("cannot write to stdout: {error}"))?;
    }

    if report.clamped_any() {
        let times = report.clamped_times;
        let noun = if times == 1 { "time" } else { "times" };
        eprintln!("warning: {times} cue {noun} clamped to 00:00:00,000");
    }

    Ok(())
}

/// Back up `file`, then atomically replace it with `output`.
fn replace_in_place(file: &Path, original: &[u8], output: &str) -> Result<(), String> {
    let metadata =
        fs::metadata(file).map_err(|error| format!("cannot read {}: {error}", file.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "cannot rewrite {}: not a regular file",
            file.display()
        ));
    }

    let backup = with_suffix(file, BACKUP_SUFFIX);
    fs::write(&backup, original)
        .map_err(|error| format!("cannot write {}: {error}", backup.display()))?;

    let temp = with_suffix(file, TEMP_SUFFIX);
    fs::write(&temp, output).map_err(|error| {
        let _ = fs::remove_file(&temp);
        format!("cannot write {}: {error}", temp.display())
    })?;
    fs::rename(&temp, file).map_err(|error| {
        let _ = fs::remove_file(&temp);
        format!("cannot replace {}: {error}", file.display())
    })?;

    Ok(())
}

/// `file` with `suffix` appended to its file name: `movie.srt` + `.bak`.
fn with_suffix(file: &Path, suffix: &str) -> PathBuf {
    let mut name = file
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(suffix);
    file.with_file_name(name)
}
