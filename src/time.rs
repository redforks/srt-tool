//! Time units and two small parsers shared by both subtitle formats.
//!
//! SubRip and ASS agree on what a millisecond is, and on the `H:MM:SS` head of a
//! timestamp. They disagree on the rest: the separator before the fraction, the
//! width of the fraction, and every rule around the timestamp itself. So the
//! units and these two helpers are shared, while each format keeps its own
//! grammar.

/// Milliseconds in one second, the unit both formats count in.
pub(crate) const MILLIS_PER_SECOND: i64 = 1_000;

/// Milliseconds in one minute.
pub(crate) const MILLIS_PER_MINUTE: i64 = 60 * MILLIS_PER_SECOND;

/// Milliseconds in one hour.
pub(crate) const MILLIS_PER_HOUR: i64 = 60 * MILLIS_PER_MINUTE;

/// Split `text` into hours, minutes, and whatever follows the second colon.
///
/// Exactly two colons are accepted, because both formats write `H:MM:SS` with no
/// missing or extra one. What follows the second colon is format-specific.
pub(crate) fn split_hours_minutes(text: &str) -> Option<(&str, &str, &str)> {
    let mut parts = text.split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(hours), Some(minutes), Some(rest), None) => Some((hours, minutes, rest)),
        _ => None,
    }
}

/// Parse exactly `width` ASCII digits.
pub(crate) fn parse_exact_digits(text: &str, width: usize) -> Option<i64> {
    if text.len() != width || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}
