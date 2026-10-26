//! SubRip (`.srt`) timestamps, cues, and whole-file editing.

use std::fmt;
use std::str::FromStr;

use crate::offset::TimeOffset;
use crate::time::{
    parse_exact_digits, split_hours_minutes, MILLIS_PER_HOUR, MILLIS_PER_MINUTE, MILLIS_PER_SECOND,
};

/// A point in the subtitle timeline, counted in milliseconds from `00:00:00,000`.
///
/// The textual form is the SubRip form `HH:MM:SS,mmm`. Hours are printed with
/// at least two digits, so a timeline longer than 100 hours still round-trips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp {
    millis: i64,
}

impl Timestamp {
    pub const ZERO: Self = Self { millis: 0 };

    pub const fn from_millis(millis: i64) -> Self {
        Self { millis }
    }

    pub const fn as_millis(self) -> i64 {
        self.millis
    }

    /// Add `offset`, clamping at `00:00:00,000`.
    ///
    /// Returns the shifted timestamp and whether clamping happened. A negative
    /// timestamp cannot be expressed in SubRip, so it becomes zero.
    pub fn shifted(self, offset: TimeOffset) -> (Self, bool) {
        let target = self.millis.saturating_add(offset.millis());
        if target < 0 {
            (Self::ZERO, true)
        } else {
            (Self { millis: target }, false)
        }
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let millis = self.millis.max(0);
        write!(
            f,
            "{:02}:{:02}:{:02},{:03}",
            millis / MILLIS_PER_HOUR,
            (millis / MILLIS_PER_MINUTE) % 60,
            (millis / MILLIS_PER_SECOND) % 60,
            millis % MILLIS_PER_SECOND
        )
    }
}

impl FromStr for Timestamp {
    type Err = TimestampError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        parse_timestamp(text)
    }
}

/// Why a timestamp string was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimestampError {
    /// The text does not have the shape `HH:MM:SS,mmm`.
    Malformed,
    /// A component is out of range, for example minutes or seconds above 59.
    OutOfRange,
}

impl fmt::Display for TimestampError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => write!(f, "expected a timestamp like 00:01:02,500"),
            Self::OutOfRange => write!(f, "a timestamp component is out of range"),
        }
    }
}

impl std::error::Error for TimestampError {}

fn parse_timestamp(text: &str) -> Result<Timestamp, TimestampError> {
    let (hours, minutes, rest) = split_hours_minutes(text).ok_or(TimestampError::Malformed)?;
    let (seconds, millis) = rest.split_once(',').ok_or(TimestampError::Malformed)?;

    let hours = parse_exact_digits(hours, 2).ok_or(TimestampError::Malformed)?;
    let minutes = parse_exact_digits(minutes, 2).ok_or(TimestampError::Malformed)?;
    let seconds = parse_exact_digits(seconds, 2).ok_or(TimestampError::Malformed)?;
    let millis = parse_exact_digits(millis, 3).ok_or(TimestampError::Malformed)?;

    if minutes > 59 || seconds > 59 {
        return Err(TimestampError::OutOfRange);
    }

    Ok(Timestamp::from_millis(
        hours * MILLIS_PER_HOUR
            + minutes * MILLIS_PER_MINUTE
            + seconds * MILLIS_PER_SECOND
            + millis,
    ))
}

/// One subtitle block: a start and end timestamp plus one or more text lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    pub start: Timestamp,
    pub end: Timestamp,
    /// Text lines joined with `\n`, without the trailing separator blank line.
    pub text: String,
}

/// A whole subtitle file: the cues in file order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Subtitle {
    cues: Vec<Cue>,
}

impl Subtitle {
    /// Parse SubRip text.
    ///
    /// Tolerated: a UTF-8 byte-order mark, CRLF line endings, a missing final
    /// newline, and extra blank lines between cues. Everything else is strict:
    /// the cue index must be numeric, the timing line must read
    /// `HH:MM:SS,mmm --> HH:MM:SS,mmm`, and the index itself is discarded.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        let input = input.strip_prefix('\u{feff}').unwrap_or(input);
        let lines: Vec<&str> = input
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .collect();

        let mut cues = Vec::new();
        let mut index = 0;
        while index < lines.len() {
            // Blank lines between cues carry no meaning.
            if lines[index].trim().is_empty() {
                index += 1;
                continue;
            }

            let number_line = index + 1;
            let number = lines[index].trim();
            if !is_ascii_digits(number) {
                return Err(ParseError::new(
                    number_line,
                    ParseErrorKind::ExpectedIndex {
                        found: lines[index].to_string(),
                    },
                ));
            }
            index += 1;

            let Some(timing_line) = lines.get(index) else {
                return Err(ParseError::new(
                    number_line + 1,
                    ParseErrorKind::ExpectedTiming {
                        found: String::new(),
                    },
                ));
            };
            let timing_line_number = index + 1;
            let trimmed = timing_line.trim();
            if trimmed.is_empty() {
                return Err(ParseError::new(
                    timing_line_number,
                    ParseErrorKind::ExpectedTiming {
                        found: timing_line.to_string(),
                    },
                ));
            }
            let (start, end) = parse_timing_line(trimmed)
                .map_err(|kind| ParseError::new(timing_line_number, kind))?;
            index += 1;

            let text_start = index;
            while index < lines.len() && !lines[index].is_empty() {
                index += 1;
            }
            cues.push(Cue {
                start,
                end,
                text: lines[text_start..index].join("\n"),
            });
        }

        Ok(Self { cues })
    }

    pub fn cues(&self) -> &[Cue] {
        &self.cues
    }

    pub fn cues_mut(&mut self) -> &mut [Cue] {
        &mut self.cues
    }

    pub fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }

    /// Shift every cue timestamp by `offset`, clamping at zero.
    pub fn shift(&mut self, offset: TimeOffset) -> ShiftReport {
        let mut report = ShiftReport::default();
        for cue in &mut self.cues {
            let (start, start_clamped) = cue.start.shifted(offset);
            let (end, end_clamped) = cue.end.shifted(offset);
            cue.start = start;
            cue.end = end;
            report.clamped_times += usize::from(start_clamped) + usize::from(end_clamped);
        }
        report
    }

    /// Render the canonical SubRip form.
    ///
    /// Cues are numbered from 1 in file order, timestamps are normalized, line
    /// endings are `\n`, and the file ends with one blank line. Cue text lines
    /// are emitted verbatim.
    pub fn to_canonical_string(&self) -> String {
        let mut out = String::new();
        for (position, cue) in self.cues.iter().enumerate() {
            out.push_str(&format!(
                "{}\n{} --> {}\n{}\n\n",
                position + 1,
                cue.start,
                cue.end,
                cue.text
            ));
        }
        out
    }
}

impl fmt::Display for Subtitle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_canonical_string())
    }
}

/// What a shift changed, whichever format it was applied to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShiftReport {
    /// How many timestamps were pinned at `00:00:00,000`.
    pub clamped_times: usize,
    /// How many timestamps were rounded to a whole centisecond on writing.
    ///
    /// Rounding is a property of the written form, not of the model: an ASS
    /// timestamp holds centiseconds, while SubRip holds milliseconds, so a
    /// SubRip shift never rounds and always reports zero here.
    pub rounded_times: usize,
}

impl ShiftReport {
    pub fn clamped_any(self) -> bool {
        self.clamped_times > 0
    }

    pub fn rounded_any(self) -> bool {
        self.rounded_times > 0
    }
}

/// A syntax error in an SRT file, located at a 1-based line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub kind: ParseErrorKind,
}

impl ParseError {
    fn new(line: usize, kind: ParseErrorKind) -> Self {
        Self { line, kind }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.kind)
    }
}

impl std::error::Error for ParseError {}

/// The specific syntax problem found at [`ParseError::line`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// A cue must start with a numeric index.
    ExpectedIndex { found: String },
    /// The line after the index must be the timing line.
    ExpectedTiming { found: String },
    /// The timing line must contain `-->`.
    MissingArrow { found: String },
    /// A timestamp on the timing line could not be parsed.
    InvalidTimestamp {
        found: String,
        reason: TimestampError,
    },
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedIndex { found } => {
                write!(f, "expected a numeric cue index, found {found:?}")
            }
            Self::ExpectedTiming { found } => {
                write!(f, "expected a timing line, found {found:?}")
            }
            Self::MissingArrow { found } => {
                write!(f, "expected '-->' in the timing line, found {found:?}")
            }
            Self::InvalidTimestamp { found, reason } => {
                write!(f, "invalid timestamp {found:?}: {reason}")
            }
        }
    }
}

fn parse_timing_line(line: &str) -> Result<(Timestamp, Timestamp), ParseErrorKind> {
    let (left, right) = line
        .split_once("-->")
        .ok_or_else(|| ParseErrorKind::MissingArrow {
            found: line.to_string(),
        })?;
    let start_text = left.trim();
    let end_text = right.trim();

    let start =
        start_text
            .parse::<Timestamp>()
            .map_err(|reason| ParseErrorKind::InvalidTimestamp {
                found: start_text.to_string(),
                reason,
            })?;
    let end = end_text
        .parse::<Timestamp>()
        .map_err(|reason| ParseErrorKind::InvalidTimestamp {
            found: end_text.to_string(),
            reason,
        })?;
    Ok((start, end))
}

fn is_ascii_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Subtitle {
        Subtitle::parse(text).unwrap_or_else(|error| panic!("{text:?}: {error}"))
    }

    fn parse_err(text: &str) -> ParseError {
        Subtitle::parse(text)
            .map(|subtitle| panic!("{text:?} was accepted as {subtitle:?}"))
            .unwrap_err()
    }

    #[test]
    fn timestamp_displays_in_srt_form() {
        assert_eq!(Timestamp::from_millis(0).to_string(), "00:00:00,000");
        assert_eq!(Timestamp::from_millis(1_500).to_string(), "00:00:01,500");
        assert_eq!(
            Timestamp::from_millis(3_723_004).to_string(),
            "01:02:03,004"
        );
        assert_eq!(
            Timestamp::from_millis(360_000_000).to_string(),
            "100:00:00,000"
        );
    }

    #[test]
    fn timestamp_parses_srt_form() {
        assert_eq!(
            "00:00:01,500".parse::<Timestamp>().unwrap().as_millis(),
            1_500
        );
        assert_eq!(
            "01:02:03,004".parse::<Timestamp>().unwrap().as_millis(),
            3_723_004
        );
    }

    #[test]
    fn timestamp_rejects_bad_shapes() {
        for text in [
            "00:00:01",
            "0:00:01,500",
            "00:0:01,500",
            "00:00:1,500",
            "00:00:01,50",
            "00:00:01.500",
            "",
            "aa:00:01,500",
            "00:00:01,500 ",
            "-00:00:01,500",
        ] {
            assert_eq!(
                text.parse::<Timestamp>().unwrap_err(),
                TimestampError::Malformed,
                "{text:?}"
            );
        }
    }

    #[test]
    fn timestamp_rejects_out_of_range_components() {
        for text in ["00:60:00,000", "00:00:60,000"] {
            assert_eq!(
                text.parse::<Timestamp>().unwrap_err(),
                TimestampError::OutOfRange,
                "{text:?}"
            );
        }
    }

    #[test]
    fn shifted_clamps_at_zero() {
        let (early, clamped) =
            Timestamp::from_millis(1_000).shifted(TimeOffset::from_millis(-2_000));
        assert_eq!(early, Timestamp::ZERO);
        assert!(clamped);

        let (later, clamped) = Timestamp::from_millis(1_000).shifted(TimeOffset::from_millis(500));
        assert_eq!(later, Timestamp::from_millis(1_500));
        assert!(!clamped);
    }

    #[test]
    fn parses_a_simple_file() {
        let subtitle = parse(
            "1\n00:00:01,000 --> 00:00:02,500\nHello\n\n2\n00:00:03,000 --> 00:00:04,000\nA\nB\n\n",
        );
        assert_eq!(subtitle.cues().len(), 2);
        assert_eq!(subtitle.cues()[0].text, "Hello");
        assert_eq!(subtitle.cues()[1].text, "A\nB");
        assert_eq!(subtitle.cues()[1].start, Timestamp::from_millis(3_000));
    }

    #[test]
    fn parses_crlf_and_a_byte_order_mark() {
        let subtitle = parse("\u{feff}1\r\n00:00:01,000 --> 00:00:02,000\r\nHello\r\n\r\n");
        assert_eq!(subtitle.cues().len(), 1);
        assert_eq!(subtitle.cues()[0].text, "Hello");
    }

    #[test]
    fn parses_without_a_final_newline() {
        let subtitle = parse("1\n00:00:01,000 --> 00:00:02,000\nHello");
        assert_eq!(subtitle.cues().len(), 1);
        assert_eq!(subtitle.cues()[0].text, "Hello");
    }

    #[test]
    fn skips_extra_blank_lines() {
        let subtitle = parse(
            "\n\n1\n00:00:01,000 --> 00:00:02,000\nHello\n\n\n\n2\n00:00:03,000 --> 00:00:04,000\nBye\n",
        );
        assert_eq!(subtitle.cues().len(), 2);
    }

    #[test]
    fn accepts_an_empty_text_body() {
        let subtitle =
            parse("1\n00:00:01,000 --> 00:00:02,000\n\n2\n00:00:03,000 --> 00:00:04,000\nB\n");
        assert_eq!(subtitle.cues().len(), 2);
        assert_eq!(subtitle.cues()[0].text, "");
        assert_eq!(subtitle.cues()[1].text, "B");
    }

    #[test]
    fn parses_an_empty_file() {
        assert!(parse("").is_empty());
        assert!(parse("\n\n").is_empty());
    }

    #[test]
    fn rejects_a_non_numeric_index() {
        let error = parse_err("one\n00:00:01,000 --> 00:00:02,000\nHello\n");
        assert_eq!(error.line, 1);
        assert!(matches!(error.kind, ParseErrorKind::ExpectedIndex { .. }));
    }

    #[test]
    fn rejects_a_missing_timing_line() {
        let error = parse_err("1\n");
        assert_eq!(error.line, 2);
        assert!(matches!(error.kind, ParseErrorKind::ExpectedTiming { .. }));
    }

    #[test]
    fn rejects_a_timing_line_without_an_arrow() {
        let error = parse_err("1\n00:00:01,000 00:00:02,000\nHello\n");
        assert_eq!(error.line, 2);
        assert!(matches!(error.kind, ParseErrorKind::MissingArrow { .. }));
    }

    #[test]
    fn rejects_a_bad_timestamp_with_its_line_number() {
        let error = parse_err("1\n00:00:01,000 --> 00:0x:02,000\nHello\n");
        assert_eq!(error.line, 2);
        assert!(matches!(
            error.kind,
            ParseErrorKind::InvalidTimestamp { .. }
        ));
    }

    #[test]
    fn renders_canonical_output() {
        let subtitle = parse("7\r\n00:00:01,000 --> 00:00:02,500\r\nHello\r\n\r\n9\r\n00:00:03,000 --> 00:00:04,000\r\nWorld");
        assert_eq!(
            subtitle.to_canonical_string(),
            "1\n00:00:01,000 --> 00:00:02,500\nHello\n\n2\n00:00:03,000 --> 00:00:04,000\nWorld\n\n"
        );
    }

    #[test]
    fn canonical_output_round_trips() {
        let original =
            "1\n00:00:01,000 --> 00:00:02,500\nHello\n\n2\n00:00:03,000 --> 00:00:04,000\nA\nB\n\n";
        let subtitle = parse(original);
        assert_eq!(subtitle.to_canonical_string(), original);
        assert_eq!(parse(&subtitle.to_canonical_string()), subtitle);
    }

    #[test]
    fn shift_moves_every_timestamp_and_counts_clamps() {
        let mut subtitle = parse(
            "1\n00:00:00,500 --> 00:00:01,000\nEarly\n\n2\n00:00:03,000 --> 00:00:04,000\nLater\n\n",
        );

        let report = subtitle.shift(TimeOffset::from_millis(-2_000));

        assert_eq!(report.clamped_times, 2);
        assert_eq!(
            report.rounded_times, 0,
            "a SubRip timestamp holds milliseconds exactly, so nothing rounds"
        );
        assert_eq!(subtitle.cues()[0].start, Timestamp::ZERO);
        assert_eq!(subtitle.cues()[0].end, Timestamp::ZERO);
        assert_eq!(subtitle.cues()[1].start, Timestamp::from_millis(1_000));
        assert_eq!(subtitle.cues()[1].end, Timestamp::from_millis(2_000));
    }

    #[test]
    fn shift_forward_reports_no_clamps() {
        let mut subtitle = parse("1\n00:00:01,000 --> 00:00:02,000\nHello\n\n");
        let report = subtitle.shift(TimeOffset::from_millis(1_000));
        assert!(!report.clamped_any());
        assert_eq!(subtitle.cues()[0].start, Timestamp::from_millis(2_000));
    }
}
