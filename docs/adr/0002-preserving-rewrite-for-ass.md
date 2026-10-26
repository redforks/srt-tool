# Preserving rewrite for ASS files

ADR-0001 makes every write a canonical re-serialization. That is safe for
SubRip, where a file holds nothing but cue indices, timings, and text. An ASS
file holds far more: `[Script Info]`, `[V4+ Styles]`, font and graphic
sections, `Comment:` lines, and the author's own `Format:` field order. A
canonical writer would have to model all of it to keep it, and would still
reorder and renormalize the parts it modelled.

So `srt-tool` writes ASS files by a preserving rewrite instead: it locates the
`Dialogue:` lines in `[Events]`, replaces the two timestamp fields in each one,
and copies every other byte of the file unchanged. Line endings, section order,
styles, comments, `Comment:` lines, and override tags inside cue text all
survive because they are never parsed into a model at all. ADR-0001 keeps
governing `.srt`; this ADR does not supersede it.

## Considered options

**Adopt a crate whose model round-trips ASS.** None fits. The only crates that
keep unknown sections verbatim are dead or pre-1.0 with a moving MSRV, and the
one crate offering a format-neutral cue model for both formats pulls a
dependency tree (tokio, tracing, wasm-bindgen) many times the size of this tool.
The preserving rewrite needs no model, so it needs no crate.

**Canonical re-serialization for ASS too.** Consistent with ADR-0001, and
rejected: the first `shift` on a real `.ass` file would drop styles and fonts,
which is data loss, not normalization.

## Consequences

The writer is format-specific and works on lines of text, not on a document
model, so any command that must edit cue *text* in an ASS file will not be able
to reuse it. `shift` only moves time, which is exactly the case this preserves
well.
