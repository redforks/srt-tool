# srt-tool

A small command line editor for subtitle files, written in Rust.

`srt-tool` edits subtitle timing and text in place, without a GUI. It reads and
writes two formats: SubRip (`.srt`) and Advanced SubStation Alpha (`.ass`).

## Status

Early development. The command line interface described below is the target
design and may still change.

## Features

- Read and write `.srt` files (SubRip).
- Read and write `.ass` files (Advanced SubStation Alpha, v4+).
- Convert between `.srt` and `.ass`.
- Shift every cue by a fixed offset, forward or backward.
- Scale the timeline, for example to match a different video frame rate.
- Split a file at a given time, or merge several files into one.
- Find and replace text across all cues.
- Fix common problems: overlapping cues, negative start times, empty cues,
  malformed timestamps.

## Install

Build from source with a stable Rust toolchain (1.75 or newer):

```sh
git clone https://github.com/redforks/srt-tool.git
cd srt-tool
cargo build --release
```

The binary is `target/release/srt-tool`. Copy it anywhere on your `PATH`, or
run `cargo install --path .`.

## Usage

```sh
srt-tool <command> [options] <file>
```

`srt-tool` detects the format from the file extension. Use `-f srt` or
`-f ass` to force a format.

### Shift time

Move every cue by an offset. Positive values move cues later, negative values
move them earlier.

```sh
srt-tool shift -o +1.5s movie.srt
srt-tool shift -o -00:00:02.250 movie.ass
srt-tool shift -o +250ms --in-place movie.srt
```

### Scale time

Stretch or compress the timeline. The factor is applied to every timestamp.
To convert subtitles from 23.976 fps to 25 fps, use a factor of `23.976/25`.

```sh
srt-tool scale --factor 0.95904 movie.srt
srt-tool scale --factor 1.04271 --in-place movie.ass
```

### Convert format

```sh
srt-tool convert --to ass movie.srt      # writes movie.ass
srt-tool convert --to srt movie.ass      # writes movie.srt
srt-tool convert --to ass --output sub.ass movie.srt
```

### Split and merge

```sh
srt-tool split --at 00:20:00 movie.srt   # writes movie.part1.srt and movie.part2.srt
srt-tool merge part1.srt part2.srt -o movie.srt
```

### Text edit

```sh
srt-tool replace --find "teh" --with "the" movie.srt
srt-tool replace --find "\\N" --with " " --in-place movie.ass
```

### Check and fix

```sh
srt-tool check movie.srt          # report problems, exit code 1 if any found
srt-tool fix --in-place movie.srt # repair negative times, overlaps, empty cues
```

## Command reference

| Command   | Purpose                                        |
| --------- | ---------------------------------------------- |
| `shift`   | Move all cues by a time offset.                |
| `scale`   | Multiply all timestamps by a factor.           |
| `convert` | Convert between `.srt` and `.ass`.             |
| `split`   | Split one file into two at a given time.       |
| `merge`   | Join several files into one, sorted by time.   |
| `replace` | Find and replace text in cue bodies.           |
| `check`   | Report format and timing problems.             |
| `fix`     | Repair problems that `check` reports.          |

## Time format

Options that take a time accept these forms:

| Form            | Meaning                          |
| --------------- | -------------------------------- |
| `1.5s`          | 1.5 seconds                      |
| `250ms`         | 250 milliseconds                 |
| `-2s`           | 2 seconds earlier                |
| `00:00:02.250`  | hours, minutes, seconds, millis  |
| `90`            | 90 seconds, plain number         |

## Output behaviour

By default every command writes the result to standard output, so you can pipe
it. Use `--output FILE` (or `-o FILE`) to write to a file. Use `--in-place` to
overwrite the input file; the original is kept as `FILE.bak`.

```sh
srt-tool shift -o +1s movie.srt > fixed.srt
srt-tool shift -o +1s -o fixed.srt movie.srt
```

## Exit codes

| Code | Meaning                                  |
| ---- | ---------------------------------------- |
| `0`  | Success.                                 |
| `1`  | `check` found problems.                  |
| `2`  | Bad usage: unknown option or argument.   |
| `3`  | Input file could not be read or parsed.  |

## License

MIT. See [LICENSE](LICENSE).
