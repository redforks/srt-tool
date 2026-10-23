//! Signed time offsets, the argument of the `shift` command.

use std::fmt;
use std::str::FromStr;

/// A signed duration in milliseconds, added to every cue timestamp.
///
/// The textual form always carries an explicit sign and a unit: `+500ms`,
/// `-1.5s`, `+2s`. Fractions are rounded to the nearest millisecond.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimeOffset {
    millis: i64,
}

impl TimeOffset {
    pub const fn from_millis(millis: i64) -> Self {
        Self { millis }
    }

    pub const fn millis(self) -> i64 {
        self.millis
    }

    pub const fn is_zero(self) -> bool {
        self.millis == 0
    }
}

impl FromStr for TimeOffset {
    type Err = OffsetError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        parse_offset(text)
    }
}

impl fmt::Display for TimeOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.millis < 0 { '-' } else { '+' };
        write!(f, "{sign}{}ms", self.millis.unsigned_abs())
    }
}

/// Why an offset string was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OffsetError {
    /// The offset does not start with `+` or `-`.
    MissingSign,
    /// The offset does not end with `ms` or `s`.
    MissingUnit,
    /// The number between the sign and the unit is malformed.
    InvalidNumber,
    /// The offset does not fit in a signed 64-bit millisecond count.
    OutOfRange,
}

impl fmt::Display for OffsetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSign => {
                write!(
                    f,
                    "an offset must start with '+' or '-', for example +500ms or -1.5s"
                )
            }
            Self::MissingUnit => {
                write!(
                    f,
                    "an offset must end with 'ms' or 's', for example +500ms or -1.5s"
                )
            }
            Self::InvalidNumber => write!(
                f,
                "an offset must contain a number, for example +500ms or -1.5s"
            ),
            Self::OutOfRange => write!(f, "the offset is too large"),
        }
    }
}

impl std::error::Error for OffsetError {}

const MILLIS_PER_SECOND: i128 = 1_000;

fn parse_offset(text: &str) -> Result<TimeOffset, OffsetError> {
    let (sign, rest) = match text.as_bytes().first() {
        Some(b'+') => (1i128, &text[1..]),
        Some(b'-') => (-1i128, &text[1..]),
        _ => return Err(OffsetError::MissingSign),
    };

    // ASCII lowering keeps byte offsets, so the sliced number text stays valid.
    let lowered = rest.to_ascii_lowercase();
    let (number, unit_millis) = if let Some(stripped) = lowered.strip_suffix("ms") {
        (&rest[..stripped.len()], 1i128)
    } else if let Some(stripped) = lowered.strip_suffix('s') {
        (&rest[..stripped.len()], MILLIS_PER_SECOND)
    } else {
        return Err(OffsetError::MissingUnit);
    };

    let (integer_text, fraction_text) = match number.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (number, None),
    };
    if !is_digits(integer_text) {
        return Err(OffsetError::InvalidNumber);
    }

    let mut millis = integer_text
        .parse::<i128>()
        .map_err(|_| OffsetError::OutOfRange)?
        .checked_mul(unit_millis)
        .ok_or(OffsetError::OutOfRange)?;

    if let Some(fraction) = fraction_text {
        if !is_digits(fraction) {
            return Err(OffsetError::InvalidNumber);
        }
        let denominator = 10i128
            .checked_pow(fraction.len() as u32)
            .ok_or(OffsetError::OutOfRange)?;
        let numerator = fraction
            .parse::<i128>()
            .map_err(|_| OffsetError::OutOfRange)?
            .checked_mul(unit_millis)
            .ok_or(OffsetError::OutOfRange)?;
        // Round half up to the nearest millisecond.
        let fraction_millis = (numerator + denominator / 2) / denominator;
        millis = millis
            .checked_add(fraction_millis)
            .ok_or(OffsetError::OutOfRange)?;
    }

    let signed = millis.checked_mul(sign).ok_or(OffsetError::OutOfRange)?;
    let millis = i64::try_from(signed).map_err(|_| OffsetError::OutOfRange)?;
    Ok(TimeOffset::from_millis(millis))
}

/// True when `text` is non-empty and contains only ASCII digits.
fn is_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn millis(text: &str) -> i64 {
        text.parse::<TimeOffset>()
            .unwrap_or_else(|error| panic!("{text}: {error}"))
            .millis()
    }

    fn rejected(text: &str) -> OffsetError {
        text.parse::<TimeOffset>()
            .map(|offset| panic!("{text} was accepted as {offset}"))
            .unwrap_err()
    }

    #[test]
    fn parses_milliseconds() {
        assert_eq!(millis("+500ms"), 500);
        assert_eq!(millis("-500ms"), -500);
        assert_eq!(millis("+0ms"), 0);
    }

    #[test]
    fn parses_seconds() {
        assert_eq!(millis("+2s"), 2_000);
        assert_eq!(millis("-1s"), -1_000);
    }

    #[test]
    fn parses_fractions() {
        assert_eq!(millis("+1.5s"), 1_500);
        assert_eq!(millis("+0.25s"), 250);
        assert_eq!(millis("-1.5s"), -1_500);
        assert_eq!(millis("+500.5ms"), 501);
        assert_eq!(millis("+0.0005s"), 1);
    }

    #[test]
    fn units_are_case_insensitive() {
        assert_eq!(millis("+500MS"), 500);
        assert_eq!(millis("+2S"), 2_000);
    }

    #[test]
    fn rejects_a_missing_sign() {
        assert_eq!(rejected("500ms"), OffsetError::MissingSign);
        assert_eq!(rejected("1.5s"), OffsetError::MissingSign);
    }

    #[test]
    fn rejects_a_missing_unit() {
        assert_eq!(rejected("+500"), OffsetError::MissingUnit);
        assert_eq!(rejected("-2"), OffsetError::MissingUnit);
        assert_eq!(rejected("+5x"), OffsetError::MissingUnit);
    }

    #[test]
    fn rejects_malformed_numbers() {
        for text in ["+", "-"] {
            assert_eq!(rejected(text), OffsetError::MissingUnit, "{text:?}");
        }
        for text in [
            "+s",
            "+ms",
            "+.5s",
            "+1.s",
            "+1..5s",
            "++500ms",
            "+-500ms",
            "+500 ms",
            "+5_00ms",
            "+\u{0661}\u{0662}s",
        ] {
            assert_eq!(rejected(text), OffsetError::InvalidNumber, "{text:?}");
        }
        for text in [" +500ms", "\t+500ms"] {
            assert_eq!(rejected(text), OffsetError::MissingSign, "{text:?}");
        }
        assert_eq!(rejected("+500ms "), OffsetError::MissingUnit);
    }

    #[test]
    fn rejects_out_of_range_values() {
        assert_eq!(
            rejected("+999999999999999999999999s"),
            OffsetError::OutOfRange
        );
    }

    #[test]
    fn displays_with_sign_and_unit() {
        assert_eq!(TimeOffset::from_millis(1_500).to_string(), "+1500ms");
        assert_eq!(TimeOffset::from_millis(-500).to_string(), "-500ms");
        assert_eq!(TimeOffset::from_millis(0).to_string(), "+0ms");
    }
}
