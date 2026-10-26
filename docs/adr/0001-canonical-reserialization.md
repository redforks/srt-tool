# Canonical re-serialization on every write

`srt-tool` rewrites a whole subtitle file in one canonical form instead of
preserving untouched bytes: cues are renumbered from 1, timestamps are
normalized to `HH:MM:SS,mmm`, line endings become `LF`, extra blank lines
collapse, and the file ends with one blank line. Cue text lines are copied
verbatim.

We chose this over byte-preserving edits because a `shift` changes every timing
line anyway, and renumbering plus normalization makes the parser and the writer
one small, testable pair with no formatting state to carry. The cost is a large
first diff on messy files; the benefit is that output is identical for identical
content, which makes tests exact and makes a second run of the same command a
no-op. The trade-off was accepted knowingly: byte-preserving edits would need a
lossless document model, and that buys nothing until a command edits text rather
than time.

This decision covers SubRip only. See
[ADR-0002](0002-preserving-rewrite-for-ass.md): an ASS file is written by a
preserving rewrite instead, because it holds styles and sections that a
canonical writer would destroy.
