# Subtitle Editing

The language of `srt-tool`, a command line editor for subtitle files.

## Language

**Subtitle file**:
A text file holding a timed sequence of cues. Supported formats are SubRip
(`.srt`) and Advanced SubStation Alpha (`.ass`, `.ssa`).
_Avoid_: Track, subtitle stream

**Cue**:
One subtitle block: a start timestamp, an end timestamp, and the text shown between them.
_Avoid_: Subtitle, entry, item, block

**Event**:
One dialogue line as recorded in an ASS file, whose timing `shift` rewrites. An
ASS file also holds styles and script settings; an event is the only part of it
that is a cue.
_Avoid_: Line, dialogue, entry

**Timestamp**:
A point in the subtitle timeline. Its written form depends on the subtitle
format.
_Avoid_: Time code, timecode

**Override tags**:
Markup inside cue text that controls how it is displayed, such as `{\an8}` for
position. `srt-tool` copies override tags verbatim and never interprets them.
_Avoid_: Markup, formatting codes, ASS tags

**Offset**:
A signed duration given to `shift`, written with an explicit sign and a unit, such as `+500ms` or `-1.5s`.
_Avoid_: Delta, amount, shift value

**Shift**:
The operation that adds one offset to every cue's timestamps in a subtitle file.
_Avoid_: Move, delay, adjust

**Clamp**:
To pin an out-of-range timestamp at zero, the earliest timestamp a subtitle file
can express.
_Avoid_: Truncate, floor, cap

**Round**:
To move a timestamp to the nearest ten milliseconds, because an ASS timestamp
can only express centiseconds. A tie, exactly five milliseconds above a
centisecond, goes away from zero.
_Avoid_: Truncate, floor, snap

**Canonical output**:
The single SubRip form `srt-tool` writes: cues numbered from 1, normalized
timestamps, `LF` line endings, one trailing blank line.
_Avoid_: Normalized output, formatted output

**Preserving rewrite**:
The way `srt-tool` writes an ASS file: only the two timestamps of each event
change, and every other byte is left as it was.
_Avoid_: Surgical edit, in-place patch, minimal diff
