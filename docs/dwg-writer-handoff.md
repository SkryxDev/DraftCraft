# DraftCraft's own DWG writer: handoff

Continues `docs/dwg-handoff.md` (withshalom's PR #15: DraftCraft reads every DWG itself; saving
still went through acadrust). This page is the state of the writer that replaces acadrust for
saving, how to work on it, and what is left before acadrust can be removed.

Branch `dwg-writer`, in the worktree `.claude/worktrees/dwg-writer` of the main checkout. It
starts from the head of PR #15 (which contains `main` at f6f72215). Nothing is pushed.

## State

- **Saving DWG goes through DraftCraft's own writer** for AutoCAD 2000, 2004, 2010, 2013 and 2018.
  acadrust still writes AutoCAD 2007 files, and every version when
  `DRAFTCRAFT_DWG_WRITER=acadrust` is set (`crates/io/src/lib.rs`, `write_dwg`).
- `cargo test -p draftcraft-io --no-fail-fast`: the `it` suite has 554 passing and 26 failing
  (4 of them failed before this work, by design: `acis_data_store` × 2, `dynblock_read` × 2).
  The failures are the types listed under "What is left". Not run yet: `cargo xtask ci`, the
  other crates' tests, the wasm build.
- Not verified in AutoCAD itself (no AutoCAD here). What stands in for it is below.

## How it is built

The writer encodes the drawing's **records**: the same groups the DXF writer produces
(`dxf_write::dwg_records`, typed values, numbers exact), one record to one DWG object. So a DWG
holds what the DXF of the same drawing holds, and what a command adds to one is in the other.

| Path | What |
|---|---|
| `crates/dwg/src/compose/object.rs` | `ObjectWriter`: one object, field by field, in every version (the mirror of `file::ObjectReader`) |
| `crates/dwg/src/compose/sections.rs` | header variables, classes, object map |
| `crates/dwg/src/compose/file.rs` | `compose(Parts)`: the file laid out for 2000 (flat) and 2004/2010/2013/2018 (paged, through `write::container::pack`) |
| `crates/dwg/src/compose/r2007.rs` | AutoCAD 2007 container: **a stub that returns "unsupported"** |
| `crates/io/src/dwg/write/rec.rs` | the records: `Doc`, `Rec`, `Groups` |
| `crates/io/src/dwg/write/mod.rs` | `Cx` (names to handles, classes, structure), `encode`, `write` |
| `crates/io/src/dwg/write/common.rs` | extended data, an entity's common properties, who owns what, entity links (2000), anonymous block names |
| `crates/io/src/dwg/write/tables.rs` | control objects and table records |
| `crates/io/src/dwg/write/entities.rs` | BLOCK, ENDBLK, LINE, POINT, CIRCLE, ARC, ELLIPSE, SOLID, TRACE, 3DFACE, XLINE, RAY, TEXT, ATTRIB, ATTDEF (single line), LWPOLYLINE, INSERT, MINSERT, POLYLINE, VERTEX, SEQEND |
| `crates/io/src/dwg/write/annotation.rs` | MTEXT, DIMENSION, SPLINE, HATCH, LEADER, TOLERANCE, VIEWPORT, MLINE, IMAGE, WIPEOUT |
| `crates/io/src/dwg/write/objects.rs` | DICTIONARY, ACDBDICTIONARYWDFLT, XRECORD, ACDBPLACEHOLDER, DICTIONARYVAR, SCALE, GROUP, LAYOUT, MLINESTYLE, WIPEOUTVARIABLES, and (through withshalom's codec `read::styles`, AutoCAD 2013 and later only) TABLESTYLE, MLEADERSTYLE, MATERIAL, VISUALSTYLE, section and detail view styles, DIMASSOC, IMAGEDEF, IMAGEDEF_REACTOR, RASTERVARIABLES |
| `crates/io/src/dwg/write/header.rs` | header variables from their DXF form to DWG's |

A record with no encoding is counted by type (`Unwritten`) and reported, as before.

## How it is checked without AutoCAD

`crates/io/tests/data/mi201` holds one drawing saved by AutoCAD as DWG and as DXF in every
version, with the same handles. AutoCAD's DXF record, encoded by the writer, must be AutoCAD's DWG
object of the same handle.

```bash
cargo test -p draftcraft-io --lib dwg::write::tests::probe -- --ignored --nocapture
```

```bash
DWG_PROBE_VER=r2004 DWG_PROBE=LAYER,TEXT cargo test -p draftcraft-io --lib dwg::write::tests::probe -- --ignored --nocapture
```

The first prints, per version and type, how many objects are the same bytes, how many differ
only in the last digits of a number (AutoCAD's DXF holds 16 digits), how many differ, and how
many AutoCAD's DWG doesn't hold at all. The second prints the differing objects of the types
named, with their groups and both byte strings. `probe_header` does the same for header values.

Today every type written is the same as AutoCAD's in all six versions, except three records
whose content differs between AutoCAD's own two files (a save GUID, an entry and an extended
data item AutoCAD writes only in DXF). In `crates/dwg`: the header section, the classes and the
object map of each of AutoCAD's files, read to values and written again, are AutoCAD's bytes
(`cargo test -p draftcraft-dwg compose`); so is the second header of the 2000 file.

What AutoCAD does that the specification doesn't say, found this way: an object's reactor count
and dictionary flag come before its own fields; the last byte is filled with 1 bits; 8-bit text
ends with a 0 its length counts; the "referenced" bit of a table record is always set before
2007; anonymous blocks are stored unnumbered (`*D`); a 2000 file declares ACDBPLACEHOLDER and
LAYOUT as classes; in a 2000 file BLOCK and ENDBLK name null neighbours and the others use the
short reference forms; a TEXT's extrusion is written in full; a dimension's flag byte has bit 3
set; an AutoCAD 2018 MTEXT ends with a null reference.

## What is left, in order

1. **acadrust can't read one of our files.** `crates/io/tests/dwg_required_dictionaries.rs`
   (`an_ac1018_dwg_gets_the_variable_dictionary_autocad_always_writes`) reads our 2004 file with
   acadrust's reader and gets "Truncated DWG stream at bit 3915". Find the object (most likely
   one our own reader is lenient about) before anything else: a second reader disagreeing is
   the best warning there is that AutoCAD may too.
2. **Types with no encoding yet** (each has failing tests that name it):
   ACAD_TABLE (`roundtrip::dwg::extended::tables`, `dwg_table_cell_height` × 7), MULTILEADER,
   multiline ATTRIB and ATTDEF, HELIX, OLE2FRAME, REGION / 3DSOLID / BODY as text, underlays
   (PDFUNDERLAY and its definition), SPATIAL_FILTER (block clips), SORTENTSTABLE, and TABLESTYLE,
   MLEADERSTYLE and DIMASSOC before AutoCAD 2013 (mirror `crates/io/src/dwg/objects.rs`; the
   codec in `read::styles` starts at 2013). Whatever the reader carries as raw groups
   (`crates/io/src/dwg/carry.rs`) needs the reverse.
3. **Fit-point splines** (`roundtrip::dwg::extended::fit_splines`): which form to store before
   2013, and the knot spacing from 2013 (chord, square root, uniform), so the curve reads back
   as drawn.
4. Tests that looked at acadrust's behaviour: `types_the_dwg_writer_cannot_write_are_reported_lost`,
   `dwg_roundtrip_keeps_extension_data`, `roundtrip::dwg::conformance` × 2,
   `handle_limits::dwg_exhausted_handles_are_preserved_or_refused`, `dxf_versions::dwg_at_every_version_reads_back`
   (2007). Decide for each whether the writer or the test changes.
5. **The AutoCAD 2007 container** (`compose/r2007.rs`): Reed-Solomon codewords and its own
   compression; the reader is `crates/dwg/src/read/r2007.rs`. The objects are already written
   in the 2007 form and match AutoCAD's.
6. The picture file browsers show (`FileInfo::preview_png` / `preview_bmp` are passed empty), the
   drawing's properties and dates (`created_iso`, `updated_iso` through chrono), and the
   read-back check on every save that `docs/dwg.md` plans.
7. **acadrust removed**: the dependency (`crates/dwg/Cargo.toml`, `crates/io/Cargo.toml`),
   `crates/dwg/src/acadrust`, the worker process (`dwg_worker` in `crates/io/src/lib.rs` and
   where the apps call it), the tests that compare with it, `NOTICE`, `ATTRIBUTION.md`,
   `Cargo.lock`, `docs/dwg.md`, `docs/dwg-handoff.md`, `docs/architecture.md`.
8. `cargo xtask ci` (fmt, clippy, tests, assets, layers, wasm). The new code has not been
   through clippy's `-D warnings` or `cargo fmt` yet. Then files for someone with AutoCAD to
   open: `DWG_FIXTURES_OUT=dir cargo test -p draftcraft-io --test it write_dwg_fixtures -- --ignored`.

## Known choices to revisit

- `crates/dwg/src/write` (withshalom's first writer, over BlackCAD's records) is still there:
  `compose` uses its container, its small sections and its style codec. Once nothing else needs
  it, what `compose` uses moves there and the rest goes.
- A LEADER's first field after its vertices ("origin") is written as zero; a hatch gradient, a
  UCS and a named view are written from the specification and the reader, with no AutoCAD file
  of them to compare with. ACadSharp's samples (MIT, not in the repository) hold most entity
  types saved by AutoCAD in every version with AutoCAD's DXF beside them: pointing the probe at
  them would settle these.
- The handles of objects only AutoCAD's DXF has (not its DWG) are referred to as if written.
  DraftCraft's own records have none of these.
