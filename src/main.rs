//! Command line interface for `srt-tool`.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use srt_tool::{AssScript, ShiftReport, Subtitle, TimeOffset};

/// Exit code for a file that cannot be read, parsed, or written, or whose
/// extension names no format this tool knows.
const EXIT_FILE_ERROR: u8 = 3;

/// The extensions `srt-tool` accepts, worded as an error message lists them.
const ACCEPTED_EXTENSIONS: &str = ".srt, .ass, or .ssa";

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

/// The subtitle format that a file extension selects.
///
/// This is a binary-level concern: the library holds one parser per format and
/// never sees a file name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubtitleFormat {
    /// SubRip, extension `.srt`.
    SubRip,
    /// Advanced SubStation Alpha, extensions `.ass` and `.ssa`.
    AdvancedSubStationAlpha,
}

impl SubtitleFormat {
    /// Choose the format from `path`'s extension, compared case-insensitively.
    ///
    /// The extension is the only signal. There is no content sniffing, even
    /// when the contents disagree with the name.
    fn from_path(path: &Path) -> Result<Self, String> {
        let Some(extension) = path.extension() else {
            return Err(format!(
                "missing file extension: expected {ACCEPTED_EXTENSIONS}"
            ));
        };
        // `Path::extension` drops the dot; keep it, so the extension is both
        // matched and reported in the form the user wrote it.
        let extension = format!(".{}", extension.to_string_lossy());
        match extension.to_ascii_lowercase().as_str() {
            ".srt" => Ok(Self::SubRip),
            ".ass" | ".ssa" => Ok(Self::AdvancedSubStationAlpha),
            _ => Err(format!(
                "unsupported file extension {extension:?}: expected {ACCEPTED_EXTENSIONS}"
            )),
        }
    }
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
    // The format is chosen before the file is read, so a name that selects no
    // format fails without touching the file and without writing any output.
    let format = SubtitleFormat::from_path(file)?;

    let original =
        fs::read(file).map_err(|error| format!("cannot read {}: {error}", file.display()))?;
    // UTF-8 is the only accepted encoding, for both formats, and decoding happens
    // here rather than in a parser: the binary already holds the bytes (it keeps
    // them for the backup), one decode then serves every format, and neither
    // parser can silently transcode what it cannot read.
    let text = std::str::from_utf8(&original).map_err(|error| {
        format!(
            "cannot read {}: the file is not valid UTF-8 (invalid byte at offset {})",
            file.display(),
            error.valid_up_to()
        )
    })?;

    let (output, report) = shift_text(format, text, offset)
        .map_err(|error| format!("cannot parse {}: {error}", file.display()))?;

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

/// Parse `text` as `format`, shift it, and render the result.
///
/// The shifted text and the report that describes the shift come back as a
/// pair: the caller writes the text, then reports on it. The two formats render
/// differently on purpose: SubRip is re-serialized canonically (ADR-0001) and ASS
/// is rewritten in place (ADR-0002).
fn shift_text(
    format: SubtitleFormat,
    text: &str,
    offset: TimeOffset,
) -> Result<(String, ShiftReport), String> {
    match format {
        SubtitleFormat::SubRip => {
            let mut subtitle = Subtitle::parse(text).map_err(|error| error.to_string())?;
            let report = subtitle.shift(offset);
            Ok((subtitle.to_canonical_string(), report))
        }
        SubtitleFormat::AdvancedSubStationAlpha => {
            let script = AssScript::parse(text).map_err(|error| error.to_string())?;
            Ok(script.rewrite(offset))
        }
    }
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
