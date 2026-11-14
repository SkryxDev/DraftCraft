# DraftCraft's own DWG writer: handoff

Continues `docs/dwg-handoff.md` (withshalom's PR #15: DraftCraft reads every DWG itself; saving
still went through acadrust). This page is the state of the writer that replaced acadrust for
saving, how to work on it, and what is left.

Branch `dwg-writer`, in the worktree `.claude/worktrees/dwg-writer` of the main checkout. It
starts from the head of PR #15 (which contains `main` at f6f72215).

## State

- **Saving DWG goes through DraftCraft's own writer in every version AutoCAD saves**: 2000, 2004,
  2007, 2010, 2013 and 2018. Every file written is read back before it is handed over
  (`check_dwg` in `crates/io/src/lib.rs`): a file that doesn't read, or reads without the
  entities it was given, is never saved.
- **acadrust is gone**: no dependency, no worker process, no environment switch, no test that
  compares with it. The tests that did now compare with DraftCraft's DXF reading of the same
  drawing, and inspect the files with the object-records reader (`draftcraft_dwg::read_bytes`),
  which is independent of the reader that opens files.
- A file carries a preview (PNG from 2013, an 8-bit bitmap before), the drawing's save dates from
  AutoCAD's universal-time variables, and AutoCAD's variable dictionary when the drawing has none.
- `cargo test -p draftcraft-io --no-fail-fast`: all pass (the `it` suite 578, 21 ignored), and
  `cargo test -p draftcraft-dwg` (101).
- **Not verified in AutoCAD itself.** A nested session was set up to open the files in the
  user's AutoCAD 2027 with computer use, and access to AutoCAD was refused twice; AutoCAD may be
  blocked in the app's Computer use settings. What stands in for it is below.

## How it is built

The writer encodes the drawing's **records**: the same groups the DXF writer produces
(`dxf_write::dwg_records`, typed values, numbers exact), one record to one DWG object. So a DWG
holds what the DXF of the same drawing holds, and what a command adds to one is in the other.

| Path | What |
|---|---|
| `crates/dwg/src/compose/object.rs` | `ObjectWriter`: one object, field by field, in every version (the mirror of `file::ObjectReader`) |
| `crates/dwg/src/compose/sections.rs` | header variables, classes, object map |
| `crates/dwg/src/compose/file.rs` | `compose(Parts)`: the file laid out for 2000 (flat) and 2004/2010/2013/2018 (paged, through `write::container::pack`) |
| `crates/dwg/src/compose/r2007/` | AutoCAD 2007 container: Reed-Solomon pages (`codes.rs`), its compression (`compress.rs`), CRC-64 checks, the page and section maps |
| `crates/io/src/dwg/write/rec.rs` | the records: `Doc`, `Rec`, `Groups` |
| `crates/io/src/dwg/write/mod.rs` | `Cx` (names to handles, classes, structure), `encode`, `write` |
| `crates/io/src/dwg/write/common.rs` | extended data, an entity's common properties, who owns what, entity links (2000), anonymous block names |
| `crates/io/src/dwg/write/tables.rs` | control objects and table records |
| `crates/io/src/dwg/write/entities.rs` | BLOCK, ENDBLK, LINE, POINT, CIRCLE, ARC, ELLIPSE, SOLID, TRACE, 3DFACE, XLINE, RAY, TEXT, ATTRIB, ATTDEF (single line), LWPOLYLINE, INSERT, MINSERT, POLYLINE, VERTEX, SEQEND |
| `crates/io/src/dwg/write/annotation.rs` | MTEXT, DIMENSION, SPLINE, HATCH, LEADER, TOLERANCE, VIEWPORT, MLINE, IMAGE, WIPEOUT |
| `crates/io/src/dwg/write/objects.rs` | DICTIONARY, ACDBDICTIONARYWDFLT, XRECORD, ACDBPLACEHOLDER, DICTIONARYVAR, SCALE, GROUP, LAYOUT, MLINESTYLE, WIPEOUTVARIABLES, TABLESTYLE, MLEADERSTYLE, MATERIAL, VISUALSTYLE, CELLSTYLEMAP, section and detail view styles, DIMASSOC, IMAGEDEF, IMAGEDEF_REACTOR, RASTERVARIABLES, SPATIAL_FILTER, PDFDEFINITION, in every version |
| `crates/io/src/dwg/write/header.rs` | header variables from their DXF form to DWG's |
| `crates/io/src/dwg/write/mleader.rs`, `table.rs`, `solid.rs`, `dynblock.rs` | multileaders, tables, ACIS solids up to 2010, dynamic block objects |
| `crates/io/src/dwg/write/variables.rs` | AutoCAD's variable dictionary, added when the drawing has none |
| `crates/io/src/dwg/write/preview.rs` | the picture file browsers and the open dialog show |

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

1. **Keep opening the files in AutoCAD**: write them with
   `DWG_FIXTURES_OUT=dir cargo test -p draftcraft-io --test it write_dwg_fixtures -- --ignored`,
   open each in AutoCAD, run AUDIT, save, and read AutoCAD's save back with DraftCraft.
   AutoCAD 2027 (November 2026) opens mi201, dimensions, tables, attributes and annotation with
   no recovery and AUDIT reports 0 errors, in the versions checked (mi201 2000 and 2004,
   dimensions 2000, tables 2010, attributes and annotation 2000), after four fixes: anonymous
   blocks stored unnumbered (`*D`, `*T`), the `AcDbSavedByObjectVersion` extended data on visual
   styles up to 2013, an empty VX table in 2000 files, and SEQENDs left out of the 2000 entity
   chain. The image files were not checked.
2. **Dynamic-block parameters and actions made in the Block Editor** are lost on a DWG save (those
   a file brought are written; `crates/engine/tests/dynblock_authoring.rs` saves only DXF).
3. Text longer than 65,535 characters (UTF-16 units from 2007, bytes before) is cut to that
   length, the most a DWG text field holds; it is not reported.
4. Reader coverage that came from files acadrust wrote (edge hatch loops, curves, mirrored text,
   table cell styles) is now covered only by DraftCraft's own files; AutoCAD's files with them
   would make it independent again.

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
