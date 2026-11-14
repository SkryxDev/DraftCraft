# DWG in DraftCraft

DraftCraft reads DWG files with its own **direct reader** (`draftcraft_io::dwg`): every object of
the file is decoded field by field, in the order the format stores it, straight into
`draftcraft-doc`'s `Drawing`. There is no DXF in between and no second model. It saves DWG with
its own **writer** (`draftcraft_io::dwg::write`), in every version AutoCAD saves from 2000 to 2018
(2000, 2004, 2007, 2010, 2013, 2018); every file written is read back before it is handed over.
No third-party DWG library is used.

## Layout

```
crates/dwg  (draftcraft-dwg: layer 0, depends on no other workspace crate)
  file.rs     DwgFile and ObjectReader: the container, the object map and each object's fields,
              read in format order (bits, handles, text, colours, extended data)
  read/       DWG R13 to 2018 and later: container (r2000, r2004, r2007), sections, classes, objects
  compose/    a DWG file laid out from its objects, header and classes, as each version
              stores it (r2000 sections, the 2004 pages, the 2007 Reed-Solomon pages in r2007/)
  records/    BlackCAD's records, for the first mapping (dwg_read) and as an independent
              reading of the files the writer makes, in tests
  bits.rs, crc.rs, lz77.rs   the format's primitives, shared by reading and writing
crates/io
  dwg/        the direct reader: DWG → Drawing
    mod.rs      the order of reading, and Unread: what was left out, by type
    header.rs   header variables
    tables.rs   application ids, text styles, linetypes, layers, dimension styles, UCSs, views,
                viewport configurations
    blocks.rs   block records, block definitions, layouts and their page setups
    entities.rs every entity type, placed in its space or block in the file's order
    table.rs    ACAD_TABLE (both forms: before 2010, and the 2010 table content)
    objects.rs  dictionaries (for names), DIMASSOC, MLINESTYLE, IMAGEDEF, PDFDEFINITION
    write/      the writer: each record of `dxf_write::dwg_records` encoded as the DWG object of
                the same meaning (header, tables, entities, objects, dynamic blocks, the preview)
  dxf_read, dxf_write   DXF ⇄ Drawing
  dwg_read              the first native mapping, over BlackCAD's records (kept for comparison)
```

The codec comes from BlackCAD (<https://github.com/withshalom/blackcad>, MIT OR Apache-2.0),
written from the Open Design Alliance's public DWG specification.

Rules the direct reader keeps:

- **One model.** Each object goes straight into the drawing's own type: no intermediate record of
  the whole file is kept.
- **Each conversion rule once.** Where a value means the same in DWG and DXF, the direct reader
  calls the DXF reader's rule rather than repeating it: hatch edges and pattern lines
  (`dxf_read::hatch`), multileaders (`dxf_mleader::build` from the same `Parsed` values),
  multilines (`dxf_mline::build`, `style_from`, `apply_styles`), tables (`table_from`), underlays
  (`dxf_underlay::reference`), regions (`dxf_region::from_polyline`), dimension overrides,
  associativity and marks (`dxf_ext`), text mirroring, page setups, spline and mesh rules.
- **Nothing silently dropped.** Every object of the file is either in the drawing or counted in
  `Unread` by type (a part of an object that waits for a later step has a key with a space:
  `LAYER plot style`, `header $X`). What was left out is in the drawing's read notes.
- **Never crash.** Every count read from a file is bounded before anything is allocated for it,
  every damaged file is an error or a drawing, and each crash or misread found gets a test.

## Choosing the reader

`DRAFTCRAFT_DWG_READER` picks the reader for every DWG opened (`draftcraft_io::read`), and
`draftcraft_io::read_dwg_with(bytes, DwgReader)` picks it for one call:

| Value | Reader |
|---|---|
| (unset, or anything else) | `Direct`: the direct reader; what it left out is in the read notes |
| `native` | the first mapping (`dwg_read`), over BlackCAD's records, for comparison |

A browser has no environment: the web build always reads directly.

## What the direct reader reads

Built one step at a time; each step's tests compare it with AutoCAD's own DXF where AutoCAD's
files hold the type, otherwise with DraftCraft's DXF reading of the same drawing, saved as DWG in
every version. (Until the writer was finished they compared with acadrust's reading, MPL-2.0,
since removed.)

| Step | What |
|---|---|
| 1 | The container (R13/R14, 2000, 2004, 2010, 2013, 2018 and later), the object map, every object accounted for |
| 2 | Header variables (all of them: 197 against AutoCAD's DXF) |
| 3 | Application ids, text styles, linetypes, layers, dimension styles, views, UCSs, viewport configurations |
| 4 | Block records and definitions, layouts with their page setups |
| 5 | LINE, POINT, CIRCLE, ARC, ELLIPSE, LWPOLYLINE, TEXT, MTEXT, INSERT, MINSERT, SOLID, TRACE, 3DFACE, XLINE, RAY, with every common property and other applications' extended data; mirrored text |
| 6 | Attributes and attribute definitions (single-line) |
| 7 | 2D and 3D polylines, polyface and polygon meshes, splines |
| 8 | Every dimension kind, with overrides, associativity (DIMASSOC) and marks |
| 9 | Hatches: polyline and edge loops (line, arc, elliptic arc, spline), patterns, gradients, associativity |
| 10 | LEADER, TOLERANCE, MULTILEADER (with styles and block content) |
| 11 | VIEWPORT, IMAGE, WIPEOUT, PDFUNDERLAY, MLINE and MLINESTYLE, ACAD_TABLE (both forms), DraftCraft's regions |
| 12a | The named-object dictionary: dictionaries, XRECORDs, scales, dictionary variables, groups, wipeout and raster variables, plot-style names, table styles, multileader styles, colour-book colours (DBCOLOR); DraftCraft's own records (constraints, layer states, plot styles, annotation); objects under a dictionary DraftCraft has no type for, kept with their DXF groups as the DXF reader keeps them |
| 12b | Extension dictionaries and reactors of entities, table records, tables and block records, with the objects under them, by the DXF reader's rule; the roles of the handles kept data refers to; an entity's material; other applications' extended data on table and block records; block clips (SPATIAL_FILTER); OLE objects (OLE2FRAME), kept as their DXF groups; HELIX; 3DSOLID, REGION and BODY whose description is text (up to AutoCAD 2010) |
| 13 | The AutoCAD 2007 (AC1021) container: the file header, page map and section map as Reed-Solomon codewords (the data bytes are taken, the parity is not checked), the 2007 compression, sections reassembled from their pages |

## The plan to full fidelity

DWG fidelity is complete when every object of a real file is in the drawing (nothing in
`Unread`) and DraftCraft writes it back. Steps 14–18 are done: the direct reader is the default,
DraftCraft writes every version itself, and acadrust is gone. What remains:

| Step | What | Shows today as |
|---|---|---|
| 12c | What a real file still leaves in the read note: the solid modeler's binary description (2004 to 2010) of 3DSOLID, REGION and BODY, and the solids' history objects (ACSH_ types; the 2013-and-later data store, ACDSDATA, is read and written: `crates/dwg/src/datastore.rs`); dynamic-block objects (the evaluation graph, parameters, actions and grips); MESH and SHAPE; draw order (SORTENTSTABLE); MATERIAL, VISUALSTYLE and the section and detail view styles before 2013; CELLSTYLEMAP, TABLECONTENT and TABLEGEOMETRY; an entity's plot style | Named in the read note of the file, by type; four io tests fail on them (`acis_data_store` × 2, `dynblock_read` × 2) |
| 14 | The switch-over: the direct reader as the default, the old paths removed | done |
| 15–16 | DraftCraft's own DWG writer (objects, sections and container, a read-back check on every save) | done |
| 17 | The other save versions: 2000, 2004, 2007, 2010, 2013 | done |
| 18 | acadrust removed | done |
| 19 | Dynamic-block parameters and actions made in the Block Editor written to DWG | done |

ARC_DIMENSION and LARGE_RADIAL_DIMENSION are read and written in DXF and DWG as AutoCAD 2027
writes them (its 2000 and 2018 files and its DXF): an arc-length dimension is `DimKind::ArcLength`
with its partial and leader flags in `DimMarks`, a jogged radius `DimKind::Radius` with
`DimMarks::jogged`. Their DWG fields are pinned by
`arc_length_and_jogged_radius_dimensions_are_laid_out_as_autocad_writes_them`.

Not yet confirmed against a file AutoCAD saved (an AutoCAD drawing with them would settle each):

- A LEADER's annotation height and width are read up to 2007 only, as acadrust reads and writes
  them (BlackCAD, which reads them in every version, can't decode 2010-and-later leaders: its own
  check fails, which agrees).
- A multiline style element's linetype before 2018 is an index into the linetype table's order
  (0x7FFF ByLayer, 0x7FFE ByBlock), as acadrust reads it.
- A hatch's elliptic edge: DWG stores ellipse parameters, DXF (by acadrust) circular angles, and
  DraftCraft's DXF reader takes them as parameters; the two agree only when the edge ends on an axis.
- A diameter dimension stores point 15 before point 10 (acadrust agrees; BlackCAD swaps them).

## Tests

```
cargo test -p draftcraft-dwg                          # the codec and its primitives
cargo test -p draftcraft-io --test it dwg_direct      # the direct reader
```

- **AutoCAD's files** (`crates/io/tests/data/mi201`: one part saved by AutoCAD in every version
  from R14 to 2018, with AutoCAD's own DXF of each): header variables, tables, blocks, layouts,
  every entity and dimension associativity are compared with AutoCAD's DXF, and every object of
  every version must be in the drawing or counted.
- **Every type in every version**: drawings with every kind of each type are saved by DraftCraft
  as DWG 2000, 2004, 2007, 2010, 2013 and 2018 and read back; every entity must match DraftCraft's
  DXF reading of the same drawing, and the object records (`draftcraft_dwg::read_bytes`, a second,
  independent reader) must hold what was written.
- **Damaged files**: byte-flipped and cut copies of AutoCAD's files and of every test drawing
  return an error or a drawing, never a panic.
- `write_dwg_fixtures` (ignored) writes every test drawing as DWG in every version, for other
  programs to read: `DWG_FIXTURES_OUT=dir cargo test -p draftcraft-io --test it write_dwg_fixtures -- --ignored`.

### The sweep and the stress drawings

`crates/io/examples/direct_sweep.rs`:

```
cargo run --release -p draftcraft-io --example direct_sweep -- out/ files…          # sweep
cargo run --release -p draftcraft-io --example direct_sweep -- --stress mixed 100000 # stress
cargo run --release -p draftcraft-io --example direct_sweep -- --read file.dwg       # time, memory
cargo run --release -p draftcraft-io --example direct_sweep -- --vs-dxf f.dwg f.dxf  # vs another program
```

- **Sweep**: AutoCAD's DWGs read directly and with the first mapping (entities per type and space, layers,
  blocks, layouts), then direct → DXF → read again; DraftCraft's drawings (DXF given) saved as DWG
  in every version and read back, every entity compared with the drawing saved.
- **Stress**: drawings built to break the reader: `mixed` (n entities of nine kinds), `polyline`
  (one of n vertices), `hatch` (n loops), `layers`, `blocks` (each with an attribute), `mtext`
  (n characters), `table` (n rows), `nested` (blocks n deep). Each is saved, read back directly,
  timed and compared; on a difference it says whether the first mapping reads the same file right.
- **Against another program's DXF** (`--vs-dxf`): the direct reading against BlackCAD's `cad dxf`
  (or AutoCAD's SAVEAS) of the same file, entity by entity (numbers within 1e-9).

### In the browser

The web build (`trunk build --release` in `apps/draftcraft-web`) reads directly too. Served
locally and driven by headless Chromium, it opened AutoCAD's MI-201 files, every test drawing of
every type and a drawing of 100,000 entities dropped on the page, with no page or console errors
(a panic in wasm shows as one), each drawn as it is on the desktop, and the read notes at the
command line. (That run was before the 2007 container was read; it refused the 2007 file with its reason.)

### Against BlackCAD

BlackCAD 0.17.0 (`cad info`, `cad dxf`) over 81 files: AutoCAD's MI-201 files, every test drawing
in every version and the stress drawings. Where both read a file, they agree, except where
BlackCAD is wrong or doesn't read the type yet:

| | BlackCAD |
|---|---|
| Diameter dimensions | points 10 and 15 swapped |
| Polygon meshes | its DXF has no M and N counts (groups 71, 72), so no reader can rebuild the faces |
| Large drawings (2004+, section map over ~48 KB) | refused: the same far-copy bug as below, in its copy of the codec |
| DIMASSOC before 2013, LEADER and MULTILEADER in 2010+, ACAD_TABLE, PDFUNDERLAY | not decoded |
| Multiline style linetypes before 2018 | all ByLayer |
| Text style names of R14 files | normalized (`STANDARD` → `Standard`); DraftCraft keeps the file's |

### Findings

Each was found by these tests, fixed, and has a test of its own:

| Found by | What was wrong | Fix |
|---|---|---|
| io tests with the DXF bridge disabled | A hatch gradient colour ByLayer or ByBlock read as black | The colour word's 0xC0/0xC1/0xC8 forms |
| The sweep (DraftCraft's apartment) | Every DIMASSOC DraftCraft's save path writes was refused: associativity lost | A point reference's subentity is 32-bit and only there when the snap names objects; intersection snaps name a second list; a point may have several references |
| Stress, `mixed` | Reading grew with the square of the entities: 100,000 in 21 s (now 0.8 s) | The style, block and linetype tables by handle made once, not per dimension; entities gathered by block record once |
| Stress, `mixed` 600,000 in 2018 | Refused: "copy of 4 bytes overflows page size" | LZ77 far copies `0001HLLL`: H adds 0x4000 to the distance, LLL is the length (the codec read four length bits and no H); only a system page over ~48 KB uses them |
| Stress, `layers` | 100,000 layers: 136 s (now 0.65 s) | Table records merged through a name index; layers known to the drawing kept in a set |
| Step 11 tests | A file with two block records called `*Paper_Space` put a layout's entities on the wrong tab | The active paper space is the one the block control names |
| Step 10 tests | Dictionaries from 2007 on, and some hatch and multileader lists, refused as too long for their record | Per-item minimum sizes that hold for the smallest real record |

Found on AutoCAD's own files while finishing steps 12 and 13 (each fixed, with a test):

| Found by | What was wrong | Fix |
|---|---|---|
| AutoCAD's *Active viewport (ACadSharp samples) | A VPORT's aspect ratio read as 2988 where the DXF has 2.716 | The record holds the view's width; the ratio is the width over the height |
| AutoCAD's MTEXT 3EC, and `mtext_caret_spaces` | `^ C` read back as `^C` | DWG text is stored as it is (a line feed is the character); the DXF escapes `^ `, `^I`, `^J` are not decoded in it |
| AutoCAD's multiline ATTDEF 796 | The embedded text's style read as the attribute's | The embedded text has its own handles (owner, layer, linetype, material) before its text style |
| MI-201 R14 | TABLESTYLE not decoded | Its colours are true-colour records in every version |
| R2000 and R2004 files | XRECORD "value runs past its data" | A string's length is 16 bits, then a code page byte (reader and writer) |
| `dim_block_resave` | Opening and saving a DWG again moved the handle seed, so each save differed | A file DraftCraft wrote names its next handle; the reader follows it, as the DXF reader does |
| MI-201 2013 and later | `header $UNKNOWN_H_R2013` in every read note | The handle AutoCAD 2013 added to the header is null in AutoCAD's files and has no DXF variable: not a variable |

Where the DWG reading and AutoCAD's DXF of the same drawing still differ by design:

- A solid's text names the date it was saved; AutoCAD's DXF writes that field empty. The DWG
  reading keeps what the file holds.
- `$XCLIPFRAME` in 2004 and 2007: the DXF holds a yes/no and keeps the value in a dictionary
  variable; the DWG header holds the value (2).

Found outside the reader:
- Opening a file that can't be read reports "invalid parameters for `open`: …" before the reason
  (the engine wraps every reading error as bad parameters).

Found in the save path DraftCraft used before its own writer (acadrust's): text longer than
65,535 characters was cut to its length modulo 65,536 (a 16-bit length). The drawing model
itself grows with the square of the layers when entities are added one by one
(`Drawing::ensure_layer` searches the list: building a drawing of 100,000 layers takes 74 s, saving
it 144 s), which the DXF reader and the editor share. The DXF reader keeps at most 100,000 hatch
loops, the DWG reader more.

## Speed

Measured 2026-11-13 on a 4-core cloud machine (release build, single runs), the direct reader
against acadrust (the DWG library DraftCraft used then) on the same files:

| Drawing | File | Direct | acadrust |
|---|---|---|---|
| 100,000 mixed entities (177,779 with the dimensions' blocks), 2018 | 5.4 MB | 0.84 s, 510 MB | 2.8 s, 1.1 GB |
| 600,000 mixed entities (1,066,664), 2018 | 32 MB | 6.5 s, 3.4 GB | 34.5 s, 6.4 GB |
| 1,000,000 mixed entities, 2000 | 143 MB | 9.5 s | |
| One polyline of 1,000,000 vertices | 13 MB | 0.07 s | |
| 50,000 blocks, each inserted with an attribute | 6.8 MB | 1.3 s | |
| A table of 10,000 rows × 20 columns | 6.3 MB | 0.66 s | |
| 10,000 layers | 0.3 MB | 0.06 s | |
| 100,000 layers (one line on each) | 2.9 MB | 0.65 s, 255 MB | |

The older `dwg_bench` example compares the first mapping (`native`) with the direct reader:
`cargo run --release -p draftcraft-io --example dwg_bench -- [entities] [--keep out.dwg]`.
