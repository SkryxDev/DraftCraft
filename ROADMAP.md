# RustDraft roadmap

RustDraft **1.1.0** is out for macOS (Apple silicon), Windows (x64 and ARM64) and Linux (x64). It is a
young project: good for 2D drafting, dimensioning and plotting, and still missing parts of what a full
CAD suite does. This page says what works today, what is known to be missing and what comes next.

Something not listed here is broken, or missing something you need?
[Open an issue](https://github.com/SkryxDev/RustDraft/issues/new) — or a pull request.

## What works today

| Area | What you get |
|---|---|
| Interface | Windows-style ribbon (10 tabs, split buttons, layer and property lists), application menu, Quick Access Toolbar, file tabs, floating command line, ViewCube and navigation bar, status bar toggles, Properties and Layers palettes, 18 languages |
| Command line | AutoCAD command names and aliases, prompts with keywords, AutoComplete, `@dx,dy`, `@d<a`, `#x,y`, direct distance entry, transparent commands, scripts |
| Draw | LINE, PLINE (arcs, widths), CIRCLE, ARC, RECTANG, POLYGON, ELLIPSE, SPLINE, CONIC (exact conic arcs, saved as rational splines), POINT, XLINE, RAY, DONUT, REVCLOUD, WIPEOUT, TEXT, MTEXT |
| Modify | ERASE, MOVE, COPY, ROTATE, SCALE, MIRROR, STRETCH, OFFSET, TRIM, EXTEND, FILLET, CHAMFER, BREAK, JOIN, EXPLODE, arrays, draw order, OVERKILL, grips |
| Precision | Object snaps, polar tracking, ortho, grid snap, dynamic input |
| Annotation | DIM commands with dimension styles and associative dimensions, multileaders, tables, TrueType and SHX fonts, right-to-left text |
| Hatches and blocks | Pattern, solid and gradient fills; blocks, attributes, the Block Editor, in-place editing, external references |
| Parametric | Geometric and dimensional constraints, AUTOCONSTRAIN, parameters with expressions |
| Layouts and output | Paper-space layouts, viewports, page setups, plot styles, PLOT and EXPORTPDF, SVG and PNG export |
| Files | DWG and DXF reading and writing, at a chosen file version; AUDIT and RECOVER; autosave and drawing recovery |
| Automation | MCP server, JSON control channel, `rustdraft-cli`, AutoLISP, scripts and alias files |

## Known gaps

- **3D** — there is no 3D modelling yet (no EXTRUDE, REVOLVE, solids or surfaces).
- **File round-trips** — some objects that RustDraft doesn't model can be lost when a DWG or DXF is
  saved. Keep a copy of drawings that come from other programs before saving over them.
- **Ribbon** — no contextual tabs yet (for example a Text Editor tab while editing MTEXT) and no
  slide-out panels; collapsed panels open a plain list of their commands.
- **Builds** — the macOS and Windows builds are not signed or notarized, there is no Linux ARM build and
  no Windows installer yet.

## Next

1. Contextual ribbon tabs and slide-out panels, so the ribbon behaves like Windows CAD in every state.
2. Keep every object through a DWG/DXF save.
3. Signed and notarized builds, a Windows installer, and a Linux ARM build.
4. The first 3D commands.

Want something higher on this list? Say so in an [issue](https://github.com/SkryxDev/RustDraft/issues).
