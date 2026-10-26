//! Advanced SubStation Alpha (`.ass`, `.ssa`) events and their preserving rewrite.
//!
//! SubRip is written canonically (ADR-0001). An ASS file cannot be: it holds
//! `[Script Info]`, styles, font sections, `Comment:` lines, and the author's own
//! `Format:` field order, and a canonical writer would drop or reorder all of
//! that. So an ASS file is written by a *preserving rewrite* (ADR-0002): only the
//! `Start` and `End` fields of each `Dialogue:` line are replaced, and every
//! other byte is copied as it was.
//!
//! That is why this module keeps byte spans into the source text instead of
//! building a document model. The byte-order mark, CRLF line endings, section
//! order, styles, `;` comments, trailing whitespace, override tags inside cue
//! text, and whether the file ends with a newline all survive because they are
//! never read into a structure at all.

use std::fmt;
use std::ops::Range;

use crate::offset::TimeOffset;
use crate::srt::{ShiftReport, Timestamp};

const MILLIS_PER_SECOND: i64 = 1_000;
const MILLIS_PER_MINUTE: i64 = 60 * MILLIS_PER_SECOND;
const MILLIS_PER_HOUR: i64 = 60 * MILLIS_PER_MINUTE;
const MILLIS_PER_CENTISECOND: i64 = 10;

/// The section an event line lives in, compared case-insensitively.
const EVENTS_SECTION: &str = "events";

/// The `[Events]` key that declares the field order.
const FORMAT_KEY: &str = "format:";

/// The `[Events]` key of a cue. `Comment:` lines share the shape but are not cues.
const DIALOGUE_KEY: &str = "dialogue:";

/// A byte-order mark, which a UTF-8 file may open with.
const BYTE_ORDER_MARK: char = '\u{feff}';

/// The events of one ASS script, located in the text they were parsed from.
///
/// Spans, not copies: [`AssScript::rewrite`] splices replacement timestamps into
/// the source text, so nothing outside the two timestamp fields of an event can
/// change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssScript<'a> {
    /// The text the spans below point into.
    text: &'a str,
    /// Every `Dialogue:` line of the first `[Events]` section, in file order.
    events: Vec<Event>,
}

/// One `Dialogue:` line: the byte spans of its timestamp fields and their values.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Event {
    /// Byte span of the `Start` field inside the script text.
    start_span: Range<usize>,
    /// Byte span of the `End` field inside the script text.
    end_span: Range<usize>,
    /// The value read from `start_span`.
    start: Timestamp,
    /// The value read from `end_span`.
    end: Timestamp,
}

impl<'a> AssScript<'a> {
    /// Parse the events of `text`.
    ///
    /// Only the first `[Events]` section is read, and only its `Dialogue:` lines:
    /// `Comment:` lines are to be copied, not moved. Everything else in the file
    /// is left to the rewrite, which copies it byte for byte.
    ///
    /// Strict, like the SubRip parser: a `[Events]` section without a usable
    /// `Format:` line cannot be read, because the field order is never assumed.
    pub fn parse(text: &'a str) -> Result<Self, AssParseError> {
        let lines = source_lines(text);
        let mut events = Vec::new();
        // The section the parser is inside, so that events elsewhere are ignored.
        let mut stage = Stage::Before;
        let mut events_header_line = 0;
        // The first `Format:` line governs; a repeated one is not a second grammar.
        let mut format: Option<EventFormat> = None;

        for line in &lines {
            if let Some(name) = section_name(line.text.trim()) {
                stage = match stage {
                    Stage::Before if name.trim().eq_ignore_ascii_case(EVENTS_SECTION) => {
                        events_header_line = line.number;
                        Stage::Inside
                    }
                    // Only the first `[Events]` section is read.
                    Stage::Before => Stage::Before,
                    Stage::Inside | Stage::After => Stage::After,
                };
                continue;
            }
            if stage != Stage::Inside {
                continue;
            }

            // `;` opens a comment line anywhere in the file.
            let content = line.text.trim_start();
            if content.trim_end().is_empty() || content.starts_with(';') {
                continue;
            }

            if let Some(payload) = value_after_key(content, FORMAT_KEY) {
                if format.is_none() {
                    format = Some(parse_format_line(line, content, payload)?);
                }
            } else if let Some(payload) = value_after_key(content, DIALOGUE_KEY) {
                let Some(format) = &format else {
                    return Err(AssParseError {
                        line: line.number,
                        kind: AssParseErrorKind::EventBeforeFormat {
                            found: content.trim_end().to_string(),
                        },
                    });
                };
                events.push(parse_event(line, content, payload, format)?);
            }
            // `Comment:` and every other key are copied, never read.
        }

        if stage != Stage::Before && format.is_none() {
            return Err(AssParseError {
                line: events_header_line,
                kind: AssParseErrorKind::MissingFormat,
            });
        }

        Ok(Self { text, events })
    }

    /// Shift every event by `offset`, keeping every other byte of the script.
    ///
    /// Clamping matches SubRip exactly: a timestamp that would fall before zero
    /// is pinned at `0:00:00.00` and counted in the returned report.
    pub fn rewrite(&self, offset: TimeOffset) -> (String, ShiftReport) {
        let mut out = String::with_capacity(self.text.len());
        // How much of the source has been copied already.
        let mut copied = 0;
        let mut report = ShiftReport::default();

        for event in &self.events {
            let (start, start_clamped) = event.start.shifted(offset);
            let (end, end_clamped) = event.end.shifted(offset);
            report.clamped_times += usize::from(start_clamped) + usize::from(end_clamped);

            // The fields are spliced in file order, whatever order the `Format:`
            // line declared. Each field carries its parsed value alongside the
            // shifted one, so an unchanged value can keep its source bytes.
            let mut fields = [
                (&event.start_span, event.start, start),
                (&event.end_span, event.end, end),
            ];
            fields.sort_by_key(|(span, _, _)| span.start);
            for (span, parsed, value) in fields {
                out.push_str(&self.text[copied..span.start]);
                if value == parsed {
                    // The value did not move, so its written form did not either:
                    // copy the bytes the author wrote instead of re-rendering
                    // them, and count nothing as rounded. Hours padded to two
                    // digits, for example, are a spelling this honest rule keeps.
                    out.push_str(&self.text[span.clone()]);
                } else {
                    // The writer is the only place that knows a centisecond was
                    // lost, so it says so and the report totals it up.
                    let (written, rounded) = write_timestamp(value.as_millis());
                    out.push_str(&written);
                    report.rounded_times += usize::from(rounded);
                }
                copied = span.end;
            }
        }

        out.push_str(&self.text[copied..]);
        (out, report)
    }
}

/// Which part of a script the parser is reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Not inside `[Events]` yet.
    Before,
    /// Inside the first `[Events]` section.
    Inside,
    /// The first `[Events]` section has ended; the rest of the file is ignored.
    After,
}

/// The field order declared by the `[Events]` `Format:` line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EventFormat {
    /// How many fields an event line must declare.
    field_count: usize,
    /// Field position of the `Start` timestamp.
    start: usize,
    /// Field position of the `End` timestamp.
    end: usize,
    /// Field position of `Text`, the one field that may hold commas itself.
    /// `None` when the `Format:` line does not declare it.
    text: Option<usize>,
}

/// One line of the source, with the byte offset of its first character.
struct SourceLine<'a> {
    /// 1-based line number, as a parse error reports it.
    number: usize,
    /// The line without its `\n` or `\r\n` terminator, and without a byte-order mark.
    text: &'a str,
    /// Byte offset of `text`'s first character inside the whole source.
    start: usize,
}

/// Split `text` into lines, keeping each line's byte offset in the source.
///
/// The offsets are what makes the rewrite preserving: every untouched byte is
/// copied by span, not re-rendered, so line endings never need normalizing.
fn source_lines(text: &str) -> Vec<SourceLine<'_>> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (index, piece) in text.split_inclusive('\n').enumerate() {
        let content = piece.strip_suffix('\n').unwrap_or(piece);
        let content = content.strip_suffix('\r').unwrap_or(content);
        lines.push(SourceLine {
            number: index + 1,
            text: content,
            start,
        });
        start += piece.len();
    }

    // A byte-order mark belongs to the file, not to the first line's content, so
    // that `[Script Info]` is still a section header on line 1.
    if let Some(first) = lines.first_mut() {
        if let Some(rest) = first.text.strip_prefix(BYTE_ORDER_MARK) {
            first.start += BYTE_ORDER_MARK.len_utf8();
            first.text = rest;
        }
    }

    lines
}

/// The name inside a `[Section]` header line.
fn section_name(trimmed: &str) -> Option<&str> {
    trimmed.strip_prefix('[')?.strip_suffix(']')
}

/// The value of `content` when it starts with `key`, compared case-insensitively.
///
/// ASS keys are case-insensitive in every reader that matters, and a file written
/// by another tool is not ours to reject over capitalization.
fn value_after_key<'a>(content: &'a str, key: &str) -> Option<&'a str> {
    let prefix = content.get(..key.len())?;
    if !prefix.eq_ignore_ascii_case(key) {
        return None;
    }
    content.get(key.len()..)
}

/// Read the `Format:` line that declares the event field order.
fn parse_format_line(
    line: &SourceLine<'_>,
    content: &str,
    payload: &str,
) -> Result<EventFormat, AssParseError> {
    let names: Vec<&str> = payload.split(',').map(str::trim).collect();
    let position = |wanted: &str| {
        names
            .iter()
            .position(|name| name.eq_ignore_ascii_case(wanted))
    };

    match (position("Start"), position("End")) {
        (Some(start), Some(end)) => Ok(EventFormat {
            field_count: names.len(),
            start,
            end,
            text: position("Text"),
        }),
        _ => Err(AssParseError {
            line: line.number,
            kind: AssParseErrorKind::UnusableFormat {
                found: content.trim_end().to_string(),
            },
        }),
    }
}

/// Read one `Dialogue:` line into the spans the rewrite will replace.
fn parse_event(
    line: &SourceLine<'_>,
    content: &str,
    payload: &str,
    format: &EventFormat,
) -> Result<Event, AssParseError> {
    // Where the payload starts inside the whole file: past the leading
    // whitespace, the key, and the whitespace after it.
    let leading = line.text.len() - content.len();
    let key_end = content.len() - payload.len();
    let payload_text = payload.trim_start();
    let payload_start = line.start + leading + key_end + (payload.len() - payload_text.len());

    let spans = field_spans(payload_text, format);
    if spans.len() < format.field_count {
        return Err(AssParseError {
            line: line.number,
            kind: AssParseErrorKind::TooFewFields {
                found: spans.len(),
                expected: format.field_count,
            },
        });
    }

    let field =
        |position: usize, which: EventField| -> Result<(Range<usize>, Timestamp), AssParseError> {
            let span = spans[position].clone();
            let found = &payload_text[span.clone()];
            let value = parse_timestamp(found).map_err(|reason| AssParseError {
                line: line.number,
                kind: AssParseErrorKind::InvalidTimestamp {
                    field: which,
                    found: found.to_string(),
                    reason,
                },
            })?;
            Ok((payload_start + span.start..payload_start + span.end, value))
        };

    let (start_span, start) = field(format.start, EventField::Start)?;
    let (end_span, end) = field(format.end, EventField::End)?;
    Ok(Event {
        start_span,
        end_span,
        start,
        end,
    })
}

/// The byte spans of the comma-separated fields of `payload`, relative to it.
///
/// At most `format.field_count` fields are returned. Only the `Text` field may
/// hold commas itself: it keeps the commas left over between the fields before
/// it and the fields after it, and the fields after it are therefore counted
/// from the end of the line. When `Text` is the last field (the usual order) or
/// is not declared, that is the same as letting the last field keep the rest. A
/// line with too few commas yields too few spans, which the caller reports as a
/// field-count error.
fn field_spans(payload: &str, format: &EventFormat) -> Vec<Range<usize>> {
    let commas: Vec<usize> = payload.match_indices(',').map(|(comma, _)| comma).collect();

    let separators: Vec<usize> = match format.text {
        Some(text) if text + 1 < format.field_count => {
            let tail = format.field_count - text - 1;
            if commas.len() >= text + tail {
                commas[..text]
                    .iter()
                    .chain(&commas[commas.len() - tail..])
                    .copied()
                    .collect()
            } else {
                commas
            }
        }
        _ => commas
            .iter()
            .take(format.field_count.saturating_sub(1))
            .copied()
            .collect(),
    };

    let mut spans = Vec::with_capacity(separators.len() + 1);
    let mut start = 0;
    for comma in separators {
        spans.push(start..comma);
        start = comma + 1;
    }
    spans.push(start..payload.len());
    spans
}

/// Parse an ASS timestamp, `H:MM:SS.cc`, into milliseconds.
///
/// Hours carry one or more digits, so a script longer than ten hours reads;
/// minutes, seconds, and centiseconds are exactly two digits each. A centisecond
/// is ten milliseconds.
fn parse_timestamp(text: &str) -> Result<Timestamp, AssTimestampError> {
    let mut parts = text.split(':');
    let (hours, minutes, rest) = match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(hours), Some(minutes), Some(rest), None) => (hours, minutes, rest),
        _ => return Err(AssTimestampError::Malformed),
    };
    let (seconds, centiseconds) = rest.split_once('.').ok_or(AssTimestampError::Malformed)?;

    if hours.is_empty() || !hours.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AssTimestampError::Malformed);
    }
    let hours = hours
        .parse::<i64>()
        .map_err(|_| AssTimestampError::OutOfRange)?;
    let minutes = parse_exact_digits(minutes, 2).ok_or(AssTimestampError::Malformed)?;
    let seconds = parse_exact_digits(seconds, 2).ok_or(AssTimestampError::Malformed)?;
    let centiseconds = parse_exact_digits(centiseconds, 2).ok_or(AssTimestampError::Malformed)?;

    if minutes > 59 || seconds > 59 {
        return Err(AssTimestampError::OutOfRange);
    }

    let rest = minutes * MILLIS_PER_MINUTE
        + seconds * MILLIS_PER_SECOND
        + centiseconds * MILLIS_PER_CENTISECOND;
    let millis = hours
        .checked_mul(MILLIS_PER_HOUR)
        .and_then(|total| total.checked_add(rest))
        .ok_or(AssTimestampError::OutOfRange)?;
    Ok(Timestamp::from_millis(millis))
}

/// Parse exactly `width` ASCII digits.
fn parse_exact_digits(text: &str, width: usize) -> Option<i64> {
    if text.len() != width || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// Render `millis` as an ASS timestamp, `H:MM:SS.cc`, and say whether rounding
/// changed the value.
///
/// This is the only place an ASS timestamp is written, and the only place where a
/// millisecond value meets a field that holds centiseconds: rounding at write
/// time, and counting the timestamps whose written value differs from the shifted
/// value, belongs here and nowhere else.
///
/// `millis` is the clamped, shifted value, so it is never negative here.
fn write_timestamp(millis: i64) -> (String, bool) {
    let millis = millis.max(0);
    let rounded = round_to_centisecond(millis);
    let text = format!(
        "{}:{:02}:{:02}.{:02}",
        rounded / MILLIS_PER_HOUR,
        (rounded / MILLIS_PER_MINUTE) % 60,
        (rounded / MILLIS_PER_SECOND) % 60,
        (rounded % MILLIS_PER_SECOND) / MILLIS_PER_CENTISECOND
    );
    (text, rounded != millis)
}

/// Round `millis` to the nearest ten milliseconds.
///
/// An ASS timestamp holds centiseconds, so this is the width a written timestamp
/// actually has. A remainder of exactly five milliseconds is a tie and rounds
/// away from zero, so `1235` becomes `1240` and never `1230` (GLOSSARY.md,
/// Round).
fn round_to_centisecond(millis: i64) -> i64 {
    let half = MILLIS_PER_CENTISECOND / 2;
    if millis < 0 {
        // Away from zero, which for a negative value means further down.
        -(millis.saturating_sub(half) / MILLIS_PER_CENTISECOND * MILLIS_PER_CENTISECOND)
    } else {
        millis.saturating_add(half) / MILLIS_PER_CENTISECOND * MILLIS_PER_CENTISECOND
    }
}

/// A syntax error in an ASS file, located at a 1-based line number.
///
/// A separate type from `srt::ParseError`, deliberately: the two grammars share
/// no rule. SubRip has cue indices, arrows, and `HH:MM:SS,mmm`; ASS has sections,
/// a declared field order, and `H:MM:SS.cc`. A shared enum would carry variants
/// that can never occur for the format in hand, and every caller would have to
/// ignore half of it. The *shape* is shared on purpose — a 1-based `line` plus a
/// `kind` that says what is wrong — so both parsers read and print the same way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssParseError {
    pub line: usize,
    pub kind: AssParseErrorKind,
}

impl fmt::Display for AssParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.kind)
    }
}

impl std::error::Error for AssParseError {}

/// The specific syntax problem found at [`AssParseError::line`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssParseErrorKind {
    /// The `[Events]` section has no `Format:` line, so its field order is unknown.
    MissingFormat,
    /// The `[Events]` `Format:` line does not name both `Start` and `End`.
    UnusableFormat { found: String },
    /// An event line appears before the `Format:` line that declares its fields.
    EventBeforeFormat { found: String },
    /// An event line has fewer fields than `Format:` declares.
    TooFewFields { found: usize, expected: usize },
    /// A `Start` or `End` field is not an ASS timestamp.
    InvalidTimestamp {
        field: EventField,
        found: String,
        reason: AssTimestampError,
    },
}

impl fmt::Display for AssParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFormat => {
                write!(f, "expected a Format: line declaring the event fields")
            }
            Self::UnusableFormat { found } => write!(
                f,
                "expected a Format: line naming both Start and End, found {found:?}"
            ),
            Self::EventBeforeFormat { found } => write!(
                f,
                "expected a Format: line before the first event, found {found:?}"
            ),
            Self::TooFewFields { found, expected } => write!(
                f,
                "expected {expected} fields in the event line, found {found}"
            ),
            Self::InvalidTimestamp {
                field,
                found,
                reason,
            } => write!(f, "invalid {field} timestamp {found:?}: {reason}"),
        }
    }
}

/// Which of the two fields a rewrite touches. Names the field in an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventField {
    Start,
    End,
}

impl fmt::Display for EventField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start => f.write_str("Start"),
            Self::End => f.write_str("End"),
        }
    }
}

/// Why an ASS timestamp string was rejected.
///
/// Not `srt::TimestampError`: both the accepted shape and the advice in the
/// message differ, because an ASS timestamp counts centiseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssTimestampError {
    /// The text does not have the shape `H:MM:SS.cc`.
    Malformed,
    /// Minutes or seconds are above 59.
    OutOfRange,
}

impl fmt::Display for AssTimestampError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => write!(f, "expected a timestamp like 0:01:02.50"),
            Self::OutOfRange => write!(f, "a timestamp component is out of range"),
        }
    }
}

impl std::error::Error for AssTimestampError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// A script with everything a preserving rewrite must not disturb: a
    /// byte-order mark, CRLF line endings, styles, a `;` comment, a `Comment:`
    /// event, override tags, trailing whitespace, and no final newline.
    const FIXTURE: &str = "\u{feff}[Script Info]\r\n\
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

    /// [`FIXTURE`] after `+1s`: the two timestamp pairs move, nothing else does.
    const FIXTURE_ONE_SECOND_LATER: &str = "\u{feff}[Script Info]\r\n\
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

    fn parse(text: &str) -> AssScript<'_> {
        AssScript::parse(text).unwrap_or_else(|error| panic!("{text:?}: {error}"))
    }

    fn parse_err(text: &str) -> AssParseError {
        AssScript::parse(text)
            .map(|script| panic!("{text:?} was accepted as {script:?}"))
            .unwrap_err()
    }

    fn rewritten(text: &str, millis: i64) -> String {
        parse(text).rewrite(TimeOffset::from_millis(millis)).0
    }

    #[test]
    fn zero_offset_is_byte_identical() {
        assert_eq!(rewritten(FIXTURE, 0), FIXTURE);
    }

    #[test]
    fn a_zero_offset_keeps_the_written_form_of_each_timestamp() {
        // The writer's own form has no zero-padded hours. A timestamp whose value
        // does not change must not be re-rendered, so these bytes survive as the
        // author wrote them.
        let text = "[Events]\n\
            Format: Layer, Start, End, Style, Text\n\
            Dialogue: 0,00:00:01.00,00:00:02.50,Default,Hello\n";

        assert_eq!(rewritten(text, 0), text);
        assert_eq!(
            rewritten(text, 500),
            "[Events]\n\
             Format: Layer, Start, End, Style, Text\n\
             Dialogue: 0,0:00:01.50,0:00:03.00,Default,Hello\n"
        );
    }

    #[test]
    fn shift_moves_both_timestamps_and_nothing_else() {
        assert_eq!(rewritten(FIXTURE, 1_000), FIXTURE_ONE_SECOND_LATER);
    }

    #[test]
    fn reads_start_and_end_positions_from_the_format_line() {
        let text = "[Events]\n\
            Format: Start, End, Layer, Style, Text\n\
            Dialogue: 0:00:01.00,0:00:02.00,0,Caption,Hello\n";

        assert_eq!(
            rewritten(text, 1_000),
            "[Events]\n\
             Format: Start, End, Layer, Style, Text\n\
             Dialogue: 0:00:02.00,0:00:03.00,0,Caption,Hello\n"
        );
    }

    #[test]
    fn does_not_assume_that_start_comes_before_end() {
        let text = "[Events]\n\
            Format: Layer, End, Start, Style, Text\n\
            Dialogue: 0,0:00:02.00,0:00:01.00,Caption,Hello\n";

        assert_eq!(
            rewritten(text, 1_000),
            "[Events]\n\
             Format: Layer, End, Start, Style, Text\n\
             Dialogue: 0,0:00:03.00,0:00:02.00,Caption,Hello\n"
        );
    }

    #[test]
    fn reads_a_v4_ssa_format_line() {
        let text = "[Events]\n\
            Format: Marked, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
            Dialogue: Marked=0,0:00:01.00,0:00:02.00,Default,,0000,0000,0000,,Hello\n";

        assert_eq!(
            rewritten(text, 500),
            "[Events]\n\
             Format: Marked, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
             Dialogue: Marked=0,0:00:01.50,0:00:02.50,Default,,0000,0000,0000,,Hello\n"
        );
    }

    #[test]
    fn reads_section_names_and_keys_case_insensitively() {
        let text = "[Events]\r\n\
            format: Layer, start, END, Style, Text\r\n\
            DIALOGUE: 0,0:00:01.00,0:00:02.00,Default,Hello\r\n";

        assert_eq!(
            rewritten(text, 1_000),
            "[Events]\r\n\
             format: Layer, start, END, Style, Text\r\n\
             DIALOGUE: 0,0:00:02.00,0:00:03.00,Default,Hello\r\n"
        );
    }

    #[test]
    fn keeps_commas_inside_the_text_field() {
        let text = "[Events]\n\
            Format: Layer, Start, End, Style, Text\n\
            Dialogue: 0,0:00:01.00,0:00:02.00,Default,Hello, world, and again\n";

        assert_eq!(
            rewritten(text, 1_000),
            "[Events]\n\
             Format: Layer, Start, End, Style, Text\n\
             Dialogue: 0,0:00:02.00,0:00:03.00,Default,Hello, world, and again\n"
        );
    }

    #[test]
    fn keeps_commas_inside_a_text_field_that_is_not_last() {
        // `Text` holds commas, and the fields declared after it are counted from
        // the end of the line, so the timestamps stay where `Format:` put them.
        let text = "[Events]\n\
            Format: Layer, Text, Start, End, Style\n\
            Dialogue: 0,Hello, world,0:00:01.00,0:00:02.00,Default\n";

        assert_eq!(
            rewritten(text, 1_000),
            "[Events]\n\
             Format: Layer, Text, Start, End, Style\n\
             Dialogue: 0,Hello, world,0:00:02.00,0:00:03.00,Default\n"
        );
    }

    #[test]
    fn comment_lines_do_not_move() {
        let text = "[Events]\n\
            Format: Layer, Start, End, Style, Text\n\
            Comment: 0,0:00:09.00,0:00:10.00,Default,Static note\n\
            Dialogue: 0,0:00:01.00,0:00:02.00,Default,Hello\n";
        let shifted = rewritten(text, 1_000);

        assert!(
            shifted.contains("Comment: 0,0:00:09.00,0:00:10.00,Default,Static note\n"),
            "{shifted}"
        );
        assert!(
            shifted.contains("Dialogue: 0,0:00:02.00,0:00:03.00,Default,Hello\n"),
            "{shifted}"
        );
    }

    #[test]
    fn a_file_without_an_events_section_is_unchanged() {
        let text = "[Script Info]\nTitle: No events here\n";
        assert_eq!(rewritten(text, 5_000), text);
    }

    #[test]
    fn an_events_section_without_dialogue_lines_is_unchanged() {
        let text = "[Events]\nFormat: Layer, Start, End, Style, Text\n\n";
        assert_eq!(rewritten(text, 5_000), text);
    }

    #[test]
    fn an_empty_file_is_unchanged() {
        assert_eq!(rewritten("", 5_000), "");
    }

    #[test]
    fn dialogue_lines_outside_the_events_section_do_not_move() {
        let text = "[Script Info]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,Odd\n";
        assert_eq!(rewritten(text, 5_000), text);
    }

    #[test]
    fn timestamps_move_by_the_offset() {
        let text = "[Events]\n\
            Format: Layer, Start, End, Style, Text\n\
            Dialogue: 0,0:00:01.00,0:00:02.50,Default,Hello\n";

        assert!(rewritten(text, 500).contains("0:00:01.50,0:00:03.00"));
        assert!(rewritten(text, -500).contains("0:00:00.50,0:00:02.00"));
    }

    #[test]
    fn hours_carry_more_than_one_digit() {
        let text = "[Events]\n\
            Format: Layer, Start, End, Style, Text\n\
            Dialogue: 0,10:02:03.40,11:00:00.00,Default,Long\n";

        assert_eq!(
            rewritten(text, 1_000),
            "[Events]\n\
             Format: Layer, Start, End, Style, Text\n\
             Dialogue: 0,10:02:04.40,11:00:01.00,Default,Long\n"
        );
    }

    #[test]
    fn centiseconds_are_ten_milliseconds() {
        let text = "[Events]\n\
            Format: Layer, Start, End, Style, Text\n\
            Dialogue: 0,0:00:01.23,0:00:02.99,Default,Hello\n";

        assert_eq!(
            rewritten(text, 0),
            text,
            "centiseconds must round-trip exactly"
        );
        assert!(rewritten(text, 10).contains("0:00:01.24,0:00:03.00"));
    }

    #[test]
    fn clamping_pins_at_zero_and_counts_each_timestamp() {
        let text = "[Events]\n\
            Format: Layer, Start, End, Style, Text\n\
            Dialogue: 0,0:00:00.50,0:00:01.00,Default,Early\n\
            Dialogue: 0,0:00:05.00,0:00:06.00,Default,Later\n";

        let (shifted, report) = parse(text).rewrite(TimeOffset::from_millis(-2_000));

        assert_eq!(report.clamped_times, 2);
        assert!(shifted.contains("Dialogue: 0,0:00:00.00,0:00:00.00,Default,Early\n"));
        assert!(shifted.contains("Dialogue: 0,0:00:03.00,0:00:04.00,Default,Later\n"));
    }

    #[test]
    fn a_shift_without_clamping_reports_nothing() {
        let (_, report) = parse(FIXTURE).rewrite(TimeOffset::from_millis(1_000));
        assert!(!report.clamped_any());
    }

    #[test]
    fn the_report_counts_the_timestamps_whose_written_value_rounds() {
        let text = "[Events]\n\
            Format: Layer, Start, End, Style, Text\n\
            Dialogue: 0,0:00:01.00,0:00:02.50,Default,Hello\n";

        let (_, report) = parse(text).rewrite(TimeOffset::from_millis(505));

        assert_eq!(report.rounded_times, 2, "both timestamps lose 5ms of 505");
        assert!(report.rounded_any());
    }

    #[test]
    fn an_offset_of_whole_centiseconds_rounds_nothing() {
        let (_, report) = parse(FIXTURE).rewrite(TimeOffset::from_millis(500));

        assert_eq!(report.rounded_times, 0);
        assert!(!report.rounded_any());
    }

    #[test]
    fn a_clamped_timestamp_is_never_counted_as_rounded() {
        // Rounding is applied to the clamped value, so the timestamp pinned at
        // zero stays a whole centisecond and only the other one rounds.
        let text = "[Events]\n\
            Format: Layer, Start, End, Style, Text\n\
            Dialogue: 0,0:00:00.50,0:00:05.00,Default,Early\n";

        let (shifted, report) = parse(text).rewrite(TimeOffset::from_millis(-1_995));

        assert_eq!(report.clamped_times, 1);
        assert_eq!(report.rounded_times, 1);
        assert_eq!(
            shifted,
            "[Events]\n\
             Format: Layer, Start, End, Style, Text\n\
             Dialogue: 0,0:00:00.00,0:00:03.01,Default,Early\n"
        );
    }

    #[test]
    fn a_missing_format_line_reports_the_events_header_line() {
        let error = parse_err(
            "[Script Info]\nTitle: Broken\n\n[Events]\nComment: 0,0:00:01.00,0:00:02.00,Default,Hello\n",
        );

        assert_eq!(error.line, 4);
        assert_eq!(error.kind, AssParseErrorKind::MissingFormat);
        assert_eq!(
            error.to_string(),
            "line 4: expected a Format: line declaring the event fields"
        );
    }

    #[test]
    fn an_event_without_any_format_line_reports_the_event_line() {
        let error = parse_err("[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,Hello\n");

        assert_eq!(error.line, 2);
        assert!(matches!(
            error.kind,
            AssParseErrorKind::EventBeforeFormat { .. }
        ));
    }

    #[test]
    fn a_usable_format_line_must_name_start_and_end() {
        let error = parse_err("[Events]\nFormat: Layer, Style, Text\n");

        assert_eq!(error.line, 2);
        assert!(matches!(
            error.kind,
            AssParseErrorKind::UnusableFormat { .. }
        ));
        assert!(error.to_string().contains("Start and End"), "{error}");
    }

    #[test]
    fn an_event_before_the_format_line_reports_its_own_line() {
        let error = parse_err(
            "[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,Hello\nFormat: Layer, Start, End, Style, Text\n",
        );

        assert_eq!(error.line, 2);
        assert!(matches!(
            error.kind,
            AssParseErrorKind::EventBeforeFormat { .. }
        ));
    }

    #[test]
    fn too_few_fields_reports_the_event_line() {
        let error = parse_err(
            "[Script Info]\n\n[Events]\nFormat: Layer, Start, End, Style, Text\nDialogue: 0,0:00:01.00,Default\n",
        );

        assert_eq!(error.line, 5);
        assert_eq!(
            error.kind,
            AssParseErrorKind::TooFewFields {
                found: 3,
                expected: 5
            }
        );
        assert_eq!(
            error.to_string(),
            "line 5: expected 5 fields in the event line, found 3"
        );
    }

    #[test]
    fn an_invalid_timestamp_reports_the_event_line_and_the_field() {
        let error = parse_err(
            "[Events]\nFormat: Layer, Start, End, Style, Text\nDialogue: 0,0:00:01.00,not a time,Default,Hello\n",
        );

        assert_eq!(error.line, 3);
        assert_eq!(
            error.kind,
            AssParseErrorKind::InvalidTimestamp {
                field: EventField::End,
                found: "not a time".to_string(),
                reason: AssTimestampError::Malformed,
            }
        );
        assert_eq!(
            error.to_string(),
            "line 3: invalid End timestamp \"not a time\": expected a timestamp like 0:01:02.50"
        );
    }

    #[test]
    fn timestamp_parsing_is_strict_about_its_shape() {
        for text in [
            "",
            "0:00:01",
            "0:00:01.5",
            "0:0:01.50",
            "0:00:1.50",
            "0:00:01,50",
            ":00:01.50",
            "0:00:01.50 ",
            "a:00:01.50",
            "-0:00:01.50",
            "0:00:01.50.00",
        ] {
            assert_eq!(
                parse_timestamp(text).unwrap_err(),
                AssTimestampError::Malformed,
                "{text:?}"
            );
        }
    }

    #[test]
    fn timestamp_parsing_rejects_out_of_range_components() {
        for text in ["0:60:00.00", "0:00:60.00"] {
            assert_eq!(
                parse_timestamp(text).unwrap_err(),
                AssTimestampError::OutOfRange,
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_timestamp_wider_than_the_timeline_is_an_error() {
        assert_eq!(
            parse_timestamp("99999999999999999999:00:00.00").unwrap_err(),
            AssTimestampError::OutOfRange
        );
    }

    #[test]
    fn written_timestamps_round_to_the_nearest_centisecond() {
        let text = "[Events]\n\
            Format: Layer, Start, End, Style, Text\n\
            Dialogue: 0,0:00:01.00,0:00:02.00,Default,Hello\n";

        assert_eq!(
            rewritten(text, 15),
            "[Events]\n\
             Format: Layer, Start, End, Style, Text\n\
             Dialogue: 0,0:00:01.02,0:00:02.02,Default,Hello\n"
        );
        assert_eq!(write_timestamp(0).0, "0:00:00.00");
        assert_eq!(write_timestamp(3_723_004).0, "1:02:03.00");
        assert_eq!(write_timestamp(36_000_000).0, "10:00:00.00");
    }

    #[test]
    fn a_centisecond_tie_rounds_away_from_zero() {
        assert_eq!(write_timestamp(1_234).0, "0:00:01.23");
        assert_eq!(write_timestamp(1_235).0, "0:00:01.24");
        assert_eq!(write_timestamp(1_236).0, "0:00:01.24");
    }

    #[test]
    fn field_spans_stop_at_the_declared_field_count() {
        let format = |field_count, text| EventFormat {
            field_count,
            start: 0,
            end: 0,
            text,
        };

        assert_eq!(
            field_spans("a,b,c,d", &format(3, Some(2))),
            vec![0..1, 2..3, 4..7]
        );
        assert_eq!(field_spans("a,b", &format(5, Some(4))), vec![0..1, 2..3]);
        assert_eq!(field_spans("solo", &format(1, Some(0))), vec![0..4]);
    }
}
