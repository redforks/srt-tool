//! Editing of subtitle files from the command line.
//!
//! The library holds the domain logic: signed offsets, timestamps, cues, events,
//! and the duration-shift operation. The binary in `main.rs` is a thin shell
//! around it, and owns the one thing the domain does not: which format a file
//! extension selects.

pub mod ass;
pub mod offset;
pub mod srt;

mod time;

pub use ass::{AssParseError, AssParseErrorKind, AssScript, AssTimestampError, EventField};
pub use offset::{OffsetError, TimeOffset};
pub use srt::{Cue, ParseError, ParseErrorKind, ShiftReport, Subtitle, Timestamp, TimestampError};
