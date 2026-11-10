# DWG in DraftCraft

DraftCraft reads DWG files with its own reader, which builds the drawing straight from the file
with no DXF in between, or with acadrust (through DXF), the fallback for what its own reader
doesn't cover yet. Saving DWG still goes through acadrust.

## Layout

```
crates/dwg  (draftcraft-dwg: layer 0, depends on no other workspace crate)
  read/       DWG R13 to 2018 and later (except 2007) → one record per object
  write/      AutoCAD 2018 (AC1032) DWG from records (not used for saving yet)
  records/    the records: one plain struct per DWG object type, decoded and then dropped
  bits.rs, crc.rs, lz77.rs   the format's primitives, shared by reading and writing
  acadrust/   DWG ⇄ DXF bytes through the acadrust crate (MPL-2.0, used unmodified),
              in a child process on desktop builds
crates/io
  dxf_read, dxf_write   DXF ⇄ drawing
  dwg_read              DWG records → drawing
```

The reader, writer and records come from BlackCAD (<https://github.com/withshalom/blackcad>,
MIT OR Apache-2.0), written from the Open Design Alliance's public DWG specification.

Rules this layout keeps:

- **One model.** `draftcraft-doc`'s `Drawing` is the only drawing. The records are decoded,
  mapped by `dwg_read` and dropped; nothing keeps or edits them.
- **No file decoded twice** on a normal open, and **no DXF text** between a DWG file and the
  drawing on the native path.
- **Each conversion rule once.** Where a value means the same in DWG and DXF, `dwg_read` calls the
  DXF reader's helper rather than repeating it: `ocs_n` (OCS → WCS), `transparency_440`,
  `style_font_of` (a text style's font), `draftcraft_dxf::unescape_unicode` (`\U+XXXX` escapes)
  and `dxf_raw`'s rules for other applications' extended data.
- **`io` reads and writes both formats.** The format crates below it (`draftcraft-dxf`,
  `draftcraft-dwg`) know their bytes, never the drawing.

## Choosing the reader

`DRAFTCRAFT_DWG_READER` picks the reader for every DWG opened (`draftcraft_io::read`), and
`draftcraft_io::read_dwg_with(bytes, DwgReader)` picks it for one call:

| Value | Reader |
|---|---|
| `acadrust` (default) | acadrust through DXF, as before |
| `native` | DraftCraft's own reader; what it left out is listed in the drawing's read notes |
| `auto` | the native reading when it left nothing out, acadrust otherwise |

The default stays `acadrust` until the native reader carries everything a real file holds (below),
so opening a file never loses what it keeps today.

## What the native reader maps

- **Entities:** LINE, POINT, CIRCLE, ARC, ELLIPSE, LWPOLYLINE, TEXT, MTEXT, INSERT (without
  attributes), SOLID, TRACE, 3DFACE, XLINE, RAY, with layer, colour (ACI and true colour), linetype,
  lineweight, linetype scale, transparency, visibility, thickness, extrusion (a mirrored OCS as the
  DXF reader turns it into WCS geometry) and other applications' extended data.
- **Tables:** layers, linetypes, text styles (font, flags, annotative).
- **Blocks:** block definitions with their entities; anonymous blocks, which DWG stores unnumbered
  (`*D`), are numbered per prefix in table order as AutoCAD numbers them in DXF (`*D0`, `*D1`).
- **Layouts:** paper-space layouts in tab order, with their entities.
- **Header:** INSUNITS, MEASUREMENT, LUNITS, LUPREC, AUNITS, AUPREC, ANGBASE, ANGDIR, LTSCALE,
  TEXTSIZE, CELTSCALE, EXTMIN, EXTMAX, LIMMIN, LIMMAX, INSBASE, ACADVER.

## Not covered yet

`dwg_read::Coverage` counts everything below. Under `auto` such a file goes through acadrust.

- DIMENSION, HATCH, SPLINE, LEADER, MLEADER, POLYLINE (2D, 3D, mesh), attributes (ATTRIB, ATTDEF),
  VIEWPORT, IMAGE, WIPEOUT, TABLE, MLINE, TOLERANCE.
- Mirrored TEXT and MTEXT, entity plot styles and materials, extension dictionaries, external
  references.
- Non-graphic objects that DraftCraft keeps as foreign data: dictionaries, XRECORD, VISUALSTYLE,
  SCALE, MATERIAL, MLEADERSTYLE, TABLESTYLE, DICTIONARYVAR and the rest. AutoCAD writes these
  into every file, so `auto` falls back on AutoCAD's files until they are mapped.

Order of work: non-graphic objects into `Drawing::foreign` (then `auto` can be the default); the
missing entity types; saving DWG 2018 with the writer in `draftcraft-dwg` and a read-back check;
acadrust behind a feature, for 2007 files and older save-as versions.

## Tests

- `cargo test -p draftcraft-dwg`: the reader's and the format primitives' own tests.
- `cargo test -p draftcraft-io --test it dwg_native_read`: the native reader against the acadrust
  path. A drawing with every mapped entity type and property is saved as DWG 2000, 2004, 2010,
  2013 and 2018 and opened both ways; every entity must come out the same (handle, space, kind,
  properties), and so must the layers, linetypes, text styles, block names and layouts. Damaged
  files (prefixes, flipped bytes) must return an error or a drawing, never a panic.

`MI201_CORPUS` names a folder of real AutoCAD files (BlackCAD's `corpus/mosaiciron-mi201`:
MI-201 saved by AutoCAD from R14 to 2018). With it set, the tests above also run on those files.
Without it they skip. R14 files store no lineweights: acadrust reports 0.00 mm for them, the
native reader ByLayer (layers: Default), so R14 is compared without lineweights.

```
MI201_CORPUS=/path/to/blackcad/corpus/mosaiciron-mi201 cargo test -p draftcraft-dwg -p draftcraft-io
```

## Speed

```
cargo run --release -p draftcraft-io --example dwg_bench -- [entities] [--keep out.dwg]
```

`dwg_bench` builds a drawing of lines, arcs, circles and text (200,000 entities by default), saves
it as DWG 2018, opens it with both readers, and checks that every entity came out the same.

Measured 2026-11-09 on a 2-core cloud machine (release build, single runs):

| Drawing | acadrust | native |
|---|---|---|
| `dwg_bench`, 200,000 entities | 2,233–2,310 ms | **676–765 ms** (3.0–3.3×) |
| MI-201's entities copied 6,250 times (200,054), written by acadrust | 2,176 ms | **624 ms** |
| the same, written by BlackCAD's writer | 2,113 ms | **565 ms** |

The native time is the decode (about 0.3 s for 200,000 entities) plus building the drawing.
acadrust decodes, writes about 30 MB of DXF text, and DraftCraft's DXF reader parses it again.
