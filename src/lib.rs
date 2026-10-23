//! Editing of subtitle files from the command line.
//!
//! The library holds the domain logic: signed offsets, SRT timestamps, cues,
//! and the duration-shift operation. The binary in `main.rs` is a thin shell
//! around it.

pub mod offset;
pub mod srt;

pub use offset::{OffsetError, TimeOffset};
pub use srt::{Cue, ParseError, ParseErrorKind, ShiftReport, Subtitle, Timestamp, TimestampError};
