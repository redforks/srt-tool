# srt-tool

A small command line editor for subtitle files, written in Rust.

`srt-tool` edits subtitle timing in place, without a GUI. It reads and writes
SubRip (`.srt`) and Advanced SubStation Alpha (`.ass`, `.ssa`) files.

## Status

Early development. One command exists: `shift`. Every other subtitle operation
is still unbuilt.

## Install

Build from source with a stable Rust toolchain (1.85 or newer):

```sh
git clone https://github.com/redforks/srt-tool.git
cd srt-tool
cargo build --release
```

The binary is `target/release/srt-tool`. Copy it anywhere on your `PATH`, or
run `cargo install --path .`.

## Usage

```sh
srt-tool <command> <file> [arguments]
```

### Shift time

Move every cue by an offset. A positive offset moves cues later, a negative
offset moves them earlier.

```sh
srt-tool shift movie.srt +1.5s
srt-tool shift movie.srt -2s --in-place
```

The offset is mandatory and is always signed: it starts with `+` or `-`, then a
number, then a unit of `ms` or `s`.

| Offset   | Meaning                        |
| -------- | ------------------------------ |
| `+500ms` | 500 milliseconds later         |
| `-500ms` | 500 milliseconds earlier       |
| `+2s`    | 2 seconds later                |
| `-1.5s`  | 1.5 seconds earlier            |
| `+1500MS`| same as `+1500ms` (units are case-insensitive) |

A timestamp that would fall before `00:00:00,000` is pinned there, and one
warning line reports how many timestamps were pinned. The command still exits
with code 0.

```sh
$ srt-tool shift early.srt -2s
warning: 1 cue time clamped to 00:00:00,000
```

An ASS timestamp holds centiseconds, so a shift that lands between two of them
is rounded to the nearest one, with a tie going up: `+505ms` moves every
timestamp by `510ms`. One warning line reports how many timestamps were
rounded. The command still exits with code 0.

```sh
$ srt-tool shift movie.ass +505ms
warning: 2 timestamps rounded to 10ms
```

Rounding is applied after clamping, so a timestamp pinned at zero is never
reported as rounded, and SubRip is never rounded at all: its timestamps hold
milliseconds. Both warnings can appear in one run, clamping first.

```sh
$ srt-tool shift early.ass -1995ms
warning: 1 cue time clamped to 00:00:00,000
warning: 1 timestamp rounded to 10ms
```

## Output

By default the result goes to standard output, so you can pipe it:

```sh
srt-tool shift movie.srt +1s > fixed.srt
```

Pass `--in-place` to rewrite the input file. The original is first copied to
`FILE.bak`; an existing `FILE.bak` is overwritten. The new contents are written
to `FILE.srt-tool.tmp` and then renamed over `FILE`, so an interrupted run
leaves either the old file or the new one.

```sh
srt-tool shift movie.srt +1s --in-place
```

What the output looks like depends on the format. `shift` only moves time, and
the two formats record time differently.

| Format         | How the output is written                                                    |
| -------------- | ---------------------------------------------------------------------------- |
| `.srt`         | Canonical: cues renumbered, timestamps normalized, `LF` ([ADR-0001](docs/adr/0001-canonical-reserialization.md)) |
| `.ass`, `.ssa` | Preserving rewrite: only the two timestamps change ([ADR-0002](docs/adr/0002-preserving-rewrite-for-ass.md)) |

SubRip output is canonical, not byte-preserving. Cues are renumbered from 1,
timestamps are normalized to `HH:MM:SS,mmm`, line endings are `LF`, and the file
ends with one blank line. Cue text lines, including markup such as `<i>` or
`{\an8}`, are copied verbatim.

Advanced SubStation Alpha output is a preserving rewrite: only the `Start` and
`End` fields of each `Dialogue:` line are replaced, and every other byte is
copied unchanged. The byte-order mark, CRLF line endings, section order,
`[V4+ Styles]`, font sections, `;` comments, trailing whitespace, override tags
such as `{\an8}`, and whether the file ends with a newline all survive. Nothing
is renumbered because ASS events have no index. `Comment:` lines are not cues
and do not move.

## Input

The file extension selects the format. It is compared case-insensitively, so
`movie.SRT` is read exactly like `movie.srt`.

| Extension      | Format                    |
| -------------- | ------------------------- |
| `.srt`         | SubRip                    |
| `.ass`, `.ssa` | Advanced SubStation Alpha |

The extension is the only signal: the content is never inspected to guess a
format. Any other extension, or no extension, stops the run with exit code 3
before the file is read, so nothing is written.

```sh
$ srt-tool shift movie.txt +1s
error: unsupported file extension ".txt": expected .srt, .ass, or .ssa
```

`.srt` files are parsed strictly. A byte-order mark, CRLF line endings, a
missing final newline, and extra blank lines between cues are tolerated. A
non-numeric cue index, a missing `-->`, or a malformed timestamp is an error and
stops the run; the message names the line number.

Cue indices in a SubRip file are ignored and regenerated on output.

`.ass` and `.ssa` files are parsed strictly too. Only the first `[Events]`
section is read, and only its `Dialogue:` lines. The `Format:` line there
declares the field order, and the `Start` and `End` positions are read from it,
never assumed. A section without a usable `Format:` line, or an event line with
fewer fields than it declares, stops the run and the message names the line
number.

## Exit codes

| Code | Meaning                                                         |
| ---- | --------------------------------------------------------------- |
| `0`  | Success, including a shift that had to clamp or round times.    |
| `2`  | Bad usage: unknown flag, missing argument, bad offset.          |
| `3`  | Read, parse, or write failed, or the extension is not accepted. |

## Commands

| Command | Purpose                            |
| ------- | ---------------------------------- |
| `shift` | Move all cues by a time offset.    |

## License

MIT. See [LICENSE](LICENSE).
