# Subtitle Editing

The language of `srt-tool`, a command line editor for subtitle files.

## Language

**Subtitle file**:
A text file holding a timed sequence of cues. Today the only supported format is SubRip (`.srt`).
_Avoid_: Track, subtitle stream

**Cue**:
One subtitle block: a start timestamp, an end timestamp, and the text shown between them.
_Avoid_: Subtitle, entry, item, block

**Timestamp**:
A point in the subtitle timeline, written as `HH:MM:SS,mmm`.
_Avoid_: Time code, timecode

**Offset**:
A signed duration given to `shift`, written with an explicit sign and a unit, such as `+500ms` or `-1.5s`.
_Avoid_: Delta, amount, shift value

**Shift**:
The operation that adds one offset to every timestamp in a subtitle file.
_Avoid_: Move, delay, adjust

**Clamp**:
To pin an out-of-range timestamp at `00:00:00,000`, the earliest timestamp a subtitle file can express.
_Avoid_: Truncate, floor, cap

**Canonical output**:
The single form `srt-tool` writes: cues numbered from 1, normalized timestamps, `LF` line endings, one trailing blank line.
_Avoid_: Normalized output, formatted output
