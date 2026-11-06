# RustDraft

**RustDraft** is a free, open-source 2D/3D CAD and drafting application written in pure Rust,
with the **ribbon workspace** people know from Windows CAD tools — on macOS, Windows and Linux.

- **Ribbon on top**: Home, Insert, Annotate, Parametric, View, Manage, Output, Add-ins,
  Collaborate and Extra Tools, with large, small and split buttons (Circle ▾, Arc ▾, Trim ▾…),
  the layer list and the colour / linetype / lineweight lists. Panels shrink and collapse from
  the right when the window is narrow.
- **Application bar** with the application menu, the Quick Access Toolbar and a command search
  box; file tabs; a floating command line with AutoCAD-style command names and prompts.
- **DWG and DXF** reading and writing, layouts, blocks, dimensions, hatches, constraints and more.
- **Agent control**: an MCP server, a JSON control channel and a headless CLI.

RustDraft is based on [CADCraft](https://github.com/storytold/cadcraft) — see
[License and credits](#license-and-credits).

## Downloads

Builds for macOS (Apple silicon and Intel), Windows and Linux are on the
[Releases page](https://github.com/SkryxDev/RustDraft/releases). The builds are not signed by a
paid certificate: on macOS, right-click RustDraft.app ▸ Open the first time; on Windows, choose
"More info ▸ Run anyway" in SmartScreen.

## What works today

RustDraft is in early, fast development. Honest status (see [ROADMAP.md](ROADMAP.md) for parity
numbers):

| Area | Status |
|---|---|
| Drawing area | Model space with adaptive grid, axes, pan/zoom (wheel, middle-drag, pinch), crosshair cursor with pickbox, UCS icon, ViewCube, viewport label |
| Command line | Prompts with keywords, history, AutoComplete, aliases, `@dx,dy`, `@d<a`, `#x,y`, direct distance entry, Enter/space/right-click to repeat, transparent commands, `.scr`-style scripts |
| Draw | LINE, PLINE (arcs, widths), CIRCLE (center/radius/diameter, 2P, 3P), ARC, RECTANG (fillet/chamfer), POLYGON, ELLIPSE (+arcs), SPLINE, POINT, XLINE, RAY, DONUT, TEXT, MTEXT |
| Modify | ERASE, MOVE, COPY, ROTATE, SCALE, MIRROR, STRETCH, OFFSET, TRIM, EXTEND, FILLET, CHAMFER, BREAK, JOIN, EXPLODE, rectangular/polar ARRAY, draw order, OVERKILL |
| Precision | Object snaps (endpoint, midpoint, center, geometric center, node, quadrant, intersection, insertion, perpendicular, tangent, nearest; deferred tangent/perpendicular for the first point of a line, e.g. belt lines tangent to two circles), polar tracking, ortho, grid snap |
| Layers & properties | Layers palette and Layer Properties Manager (on/off, freeze, lock, plot, colour, linetype), layer tools (isolate, freeze, off, lock, match, previous), Properties palette with per-object editing, linetypes, lineweights, colour index and true colour |
| Annotation | All DIM* commands with full DIMSTYLE variables, overrides and every arrowhead, associative dimensions that follow geometry, MLEADER, TABLE, TrueType fonts (`.ttf`, `.otf`, `.ttc` collections), MTEXT formatting codes (fonts, heights, colours, stacked fractions), our own single-stroke drafting font |
| Hatch & blocks | Pick-point hatch boundaries with islands, pattern, solid and gradient fills from our own pattern library, BLOCK/INSERT, attributes and nested blocks |
| Files | DXF read (R12–2018, ASCII and binary) and write (R2000), including dimension styles, associativity, tables and constraints; DWG read (via the acadrust library) and write (R2000); PDF plotting, SVG and PNG export. Entities RustDraft doesn't model yet are not kept on save: see [file-format parity](docs/file-format-parity.md) |
| Layouts & plotting | Paper-space layouts, viewports (scale, lock, per-viewport layer freeze), MSPACE/PSPACE through viewports, page setups, PLOT and EXPORTPDF |
| Parametric | Geometric and dimensional constraints, AUTOCONSTRAIN, PARAMETERS with expressions, conflict detection; constraints re-solve after every edit |
| Grips | Hot grips with Space to cycle Stretch, Move, Rotate, Scale and Mirror |
| Interface languages | English, Ukrainian (Українська), Brazilian Portuguese (Português (Brasil)), German (Deutsch), Spanish (Español), Czech (Čeština), Russian (Русский), French (Français), Simplified Chinese (简体中文, with an installed CJK font), Italian (Italiano), Korean (한국어, with an installed Hangul font), Swedish (Svenska), Arabic (العربية, with an installed Arabic font), Hungarian (Magyar) and Turkish (Türkçe), live switching and a remembered Auto/system-language choice; [details](docs/localization.md) |
| Automation | MCP server, JSON control channel, `rustdraft-cli` (info, convert, run, commands, mcp) |

## Quick start

RustDraft requires Rust 1.95 or later.

```sh
git clone https://github.com/storytold/rustdraft
cd rustdraft
cargo run --release -p rustdraft -- --sample       # opens the sample drawing
```

Then try typing at the command line:

```text
line 0,0 @10,0 @0,5 c          a closed triangle
circle 5,2 1                    a circle
offset 0.25                     then pick the circle and a side
zoom e                          zoom to extents
```

The web build: `cd apps/rustdraft-web && trunk serve` (needs [trunk](https://github.com/trunk-rs/trunk)).

### Fonts

RustDraft has no fonts in its repository; it uses the TrueType/OpenType fonts installed on your
system. Builds made with the optional [craft-fonts](https://github.com/storytold/craft-fonts)
input (`CRAFT_FONTS_DIR=<absolute path to a checkout> cargo build --release`) also embed its
fonts: BIZ UDPGothic, BIZ UDMincho and Shippori Mincho (Japanese) and Noto Sans Arabic (the web
build, which has no system fonts, only BIZ UDPGothic Regular and Noto Sans Arabic). A text style
can use them by family name, and they draw characters a text's font lacks when no installed
wide-coverage CJK font has them. Without `CRAFT_FONTS_DIR` nothing is embedded. SHX
fonts (including big fonts such as `chineset.shx`) aren't read yet: text in an SHX style is drawn
with our own stroke font, and any character a text's font lacks (CJK, for example) is drawn from
an installed font that has it, preferring wide-coverage fonts such as Noto Sans CJK, Source Han
Sans, WenQuanYi, PingFang or Microsoft JhengHei. Two settings, set with `SETVAR` or (for every
run) an environment variable, steer this:

| Variable | Environment | Meaning |
|---|---|---|
| `FONTALT` | `RUSTDRAFT_FONTALT` | Font used instead of a text style font that can't be found (an SHX file, a missing TTF), by file or family name, e.g. `Noto Sans CJK TC`. Empty (default) = our stroke font. |
| `FONTFALLBACK` | `RUSTDRAFT_FONTFALLBACK` | Fonts tried first for characters a text's font lacks, comma-separated, e.g. `Noto Sans CJK TC, msjh.ttc`. |

`.` clears either one.

**Your own fonts.** Any `.ttf`, `.otf` or `.ttc` font works: install it in the system font folder
(Windows `C:\Windows\Fonts` or per-user fonts, macOS `/Library/Fonts` or `~/Library/Fonts`,
Linux and the BSDs `/usr/share/fonts`, `/usr/local/share/fonts`, `~/.local/share/fonts` or
`~/.fonts`) and name it in a text style (`STYLE`, or `style {"name":"FA","font":"Vazirmatn"}`)
by file name (`Vazirmatn-Regular.ttf`) or family name (`Vazirmatn`), or inline in MTEXT with
`\fVazirmatn;`. Restart RustDraft after installing a font: the font folders are scanned once.

**Right-to-left text (Persian, Arabic, Urdu, Hebrew).** TEXT and MTEXT lines are laid out with the
Unicode Bidirectional Algorithm (the paragraph direction comes from its first letter, so numbers
and Latin words inside Persian text keep their own order), and Arabic-script text is shaped with
OpenType (contextual letter forms, lam-alef and other ligatures, ZWNJ / ZWJ, mark positioning).
Justification (left, centre, right, MTEXT attachment and wrapping) still refers to the text's box.
Our stroke font has no Arabic letters, so Persian in a stroke-font style is drawn from an
installed font that has them: Vazirmatn, Noto Naskh Arabic, Noto Sans Arabic, Segoe UI, Tahoma,
Arial and DejaVu Sans are looked for first. To choose the font, install a free Persian font such
as [Vazirmatn](https://github.com/rastikerdar/vazirmatn) (SIL Open Font License) and either use it
in the text style or set `FONTFALLBACK` (`RUSTDRAFT_FONTFALLBACK="Vazirmatn"`). Formatting changes
inside a word (an MTEXT colour change in the middle of a Persian word) break the joining at that
point.

### Logs

The desktop app writes its `log` records to standard error and to `logs/rustdraft.log` in the
settings directory (Linux and the BSDs `~/.config/rustdraft/logs/`, or
`$XDG_CONFIG_HOME/rustdraft/logs/`; macOS `~/Library/Application Support/RustDraft/logs/`; Windows
`%APPDATA%\RustDraft\logs\`; or under `RUSTDRAFT_CONFIG_DIR`). A start from a desktop menu or the
Dock has no terminal, so this file is what to attach to a bug report. Each launch moves the
previous log to `rustdraft.1.log` (and that one to `rustdraft.2.log`), so the log of a run that
crashed survives the next start. The file stops growing at 16 MiB. `--version` writes no file.

| Variable | Effect |
|---|---|
| `RUSTDRAFT_CONFIG_DIR` | Use this directory instead of the platform settings directory (the log goes to its `logs/`); empty means no settings folder (no log file, autosave, workspace or plotters) |
| `RUST_LOG` | Log levels for standard error and the log file. Default: `info` for RustDraft's own crates, `warn` for everything else. env_logger-style directives replace that, e.g. `RUST_LOG=debug`, `RUST_LOG=warn,rustdraft_io=trace` or `RUST_LOG=info,wgpu_core=warn`; a directive ending in `*` covers every target starting with it (`rustdraft*=debug`). `rustdraft_engine=debug` also logs every command-line message. |

The logger is `apps/rustdraft/src/logging.rs`; the web build logs to the browser console instead.

## Drive it from agents, MCP and the CLI

```sh
# MCP server on stdio, headless:
rustdraft-cli mcp
# …or bridged to the running app (start it with --control 7979):
rustdraft-cli mcp --connect 127.0.0.1:7979

# One-shot headless runs:
rustdraft-cli run --sample --script 'CIRCLE 22,3 1\n' --save out.dxf --export out.png
rustdraft-cli info drawing.dxf
rustdraft-cli convert drawing.dxf drawing.svg

# Frame part of the drawing (model space, drawing units, lower-left corner first):
rustdraft-cli convert drawing.dwg detail.png --window 0,0,20000,15000
rustdraft-cli run drawing.dwg --cmd 'zoom {"mode":"window","p1":[0,0],"p2":[20000,15000]}' --export detail.png
rustdraft-cli run drawing.dwg --cmd 'select {"window":[[0,0],[20000,15000]]}' --cmd 'zoom {"mode":"object"}' --export sel.png

# Act on what an earlier --cmd made: {last} is the last handle it returned.
rustdraft-cli run --metric --cmd 'layout.set {"name":"Layout1"}' \
  --cmd 'mview {"p1":[20,20],"p2":[200,150]}' --cmd 'viewport.set {"handle":"{last}","scale":1}'
```

How image exports (PNG 2400×1600, SVG, PDF) are framed: `convert` fits the model-space extents.
`run --export` shows the current view when the script or commands changed it (`zoom`, `pan`,
`view.set`, …) and otherwise fits the extents too. `--window x0,y0,x1,y1` shows exactly that
rectangle, centred and scaled without distortion; `--view extents` or `--view current` forces
either framing. To frame a selection set, `zoom {"mode":"object"}` after selecting.

When reading a file leaves something out (records a DXF reader can't place, object records of a
damaged DWG), `info` reports it (`skipped`, `readNotes`), `convert` warns on stderr and `run`
prints the DWG notes with its log; the exit code stays 0. MCP `open` replies with the notes as
`warnings`.

MCP tools include `command_line` (type at the prompt), `execute` (any command with JSON),
`inspect_drawing`, `query_entities`, `render` (returns a PNG) and, when connected to the app,
`screenshot` and `ui_click`. The desktop app's JSON control channel is documented in
[docs/control-protocol.md](docs/control-protocol.md).

To run LISP routines, scripts and alias files, or to drive RustDraft from your own programs, see
[docs/customization.md](docs/customization.md).

`rustdraft-cli --help` documents every subcommand and flag; exit codes, streams, environment
variables and its RFC-0012 conventions are in [docs/cli.md](docs/cli.md).

## Architecture

```text
geom ─┐                       f64 geometry: arcs, bulges, splines, intersections, offsets
dxf   │  (standalone)         DXF tag reader/writer
color ┤                       colour index, true colour
doc   ┤                       drawing database, copy-on-write entity store (cheap undo)
fonts ┤ render               stroke font + TEXT/MTEXT layout │ display lists, linetypes, hatches, dims, CPU raster
io    ┤                       DXF mapping, SVG/PNG export
engine┤                       sessions, commands, command line + prompts, snaps, selection, undo
ui-egui · mcp                 the swappable egui front end · MCP server
apps: rustdraft · rustdraft-cli · rustdraft-web
```

Nothing below `ui-egui` knows about egui, so the front end can be replaced. `cargo xtask ci`
checks formatting, clippy, tests, asset attribution, the crate layering and the wasm build.

Drafting and layer-palette icon buttons use the pinned `craft-ui` button component for interaction,
accessible labels and keyboard focus. RustDraft keeps its multicolor icon painter, dark palette,
disabled tint and tool commands. The native host saves workspace visibility and toolset choices
atomically; `RUSTDRAFT_CONFIG_DIR` selects an isolated profile. Compact toolbar groups move into a
More menu, and status readouts truncate with a full-value tooltip before they reach layout tabs.
Painted toolbar and tab controls
have accessible labels and visible keyboard focus. Property controls remain application-specific.

## Accessibility

### System cursor

Over the drawing area RustDraft hides the system cursor and draws its own crosshair. Tools that follow the system cursor (Windows Magnifier set to follow or centre the mouse pointer, other screen magnifiers, screen recorders, remote desktops) lose track of it there. Turn on **View ▸ Accessibility ▸ Show System Cursor** to keep a small system crosshair visible at the centre of the drawn one, or start RustDraft with the environment variable `RUSTDRAFT_SYSTEM_CURSOR=1` (on Windows, set it once with `setx RUSTDRAFT_SYSTEM_CURSOR 1` and start RustDraft again). Agents can switch it with the control channel: `ui.set {"systemCursor": true}`.

### Animation effects

RustDraft follows the operating system's animation setting. When the system is set to show no animations, dialogs, menus and tooltips appear at once instead of fading in, collapsing sections and scroll bars change at once, and the mouse wheel scrolls lists a whole step at a time instead of gliding. Zooming the drawing with the wheel always moves a whole step; with Ctrl held (a touchpad pinch, on Windows) it glides to the new zoom only while animation effects show.

| System | Where the setting is |
|---|---|
| Windows | Settings ▸ Accessibility ▸ Visual effects ▸ Animation effects |
| macOS | System Settings ▸ Accessibility ▸ Reduce motion |
| Linux, BSD | the desktop's reduced-motion (or animations) setting, read through the XDG Desktop Portal |
| Web | the browser's `prefers-reduced-motion` |

A change to the system setting is picked up while RustDraft runs, within a few seconds. To choose for RustDraft alone, use **View ▸ Accessibility ▸ Animation Effects**: **Use System Setting** (the default), **On** or **Off**. Options ▸ Display has the same choice. Agents can set it with the control channel: `ui.set {"animations": "off"}` (`"system"`, `"on"` or `"off"`), and `ui.inspect` reports whether animations show (`animations`).

Drafting feedback is not an animation effect and stays as it is: rubber bands and previews, snap markers, the pause that acquires an object snap tracking point, the text cursor's blink, the spinner shown while a drawing opens, and pan and zoom, which follow the pointer and the wheel directly.

## License and credits

RustDraft is licensed under the MIT License or the Apache License 2.0, at your option
(`LICENSE-MIT`, `LICENSE-APACHE`).

RustDraft is a derivative of **CADCraft** (https://github.com/storytold/cadcraft), copyright (c)
2026 the ArtCraft Team and the CADCraft contributors, used under the same MIT OR Apache-2.0
licence. Their copyright notice is kept in `LICENSE-MIT` and `NOTICE`; third-party material is
listed in `NOTICE` and `ATTRIBUTION.md`. RustDraft's changes (the Windows-style ribbon
workspace, the new name and icon, and the rest of this repository's history) are by Skryx Dev.

RustDraft is an independent project. It is not made, sponsored or endorsed by the ArtCraft Team
or the CADCraft project, and it is not affiliated with, sponsored by or endorsed by Autodesk, Inc.
AutoCAD and DWG are trademarks of Autodesk, Inc.; they are named only to describe compatibility.
All of RustDraft's icons are drawn in code; no Autodesk icons, fonts or artwork are used.
