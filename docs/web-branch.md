# The web branch: DraftCraft in the browser, with a phone viewer

The `web` branch is `main` plus the browser site published on Vercel (project `draftcraftwebapp`):

- **On a computer or tablet** the site is the full DraftCraft (`apps/draftcraft-web`, `index.html`),
  exactly the app on `main`.
- **On a phone** the site is the **viewer** (`apps/draftcraft-web/src/viewer`, `viewer.html`): a
  separate, much smaller binary that only looks at drawings. It opens DWG and DXF files (and the SHX
  fonts they use) and has
  - one-finger pan with inertia, pinch zoom, double tap to zoom in, two-finger tap to zoom out;
  - Fit (zoom to the drawing, or to the paper of a layout);
  - layers: show or hide each one, show only one, all on, all off, back to the file's states, search;
  - layouts: model space and every paper layout;
  - text search: finds TEXT and MTEXT in the current space and zooms to a match;
  - measuring: distance (length, ΔX, ΔY, angle) and area (area, perimeter), snapping to endpoints and
    corners, midpoints and the nearest point on a line; a finger dragging aims a reticle above it;
  - dark or light background and lineweight display;
  - a link to the full app.
  Nothing is ever edited or saved: the viewer is read only.

## Which one a device gets

`vercel.json` sends phones (by user agent: iPhone, Android phones and the like; not tablets) from `/`
to `/viewer.html` before anything downloads. On any other host the full app checks at start
(`src/phone.rs`: a phone user agent, or a small touch screen) and moves to the viewer. `?desktop`
keeps the full app on a phone; `?mobile` opens the viewer anywhere.

## Staying in step with main

- `.github/workflows/web-sync.yml` (on `main`) merges every push to `main` into `web` and then starts
  `web-deploy.yml`. If the merge conflicts, the run fails and opens an issue; merge `main` into `web`
  by hand and push.
- `.github/workflows/web-deploy.yml` (on `web`) runs the web crate's tests, builds both pages with
  Trunk and publishes them with the Vercel CLI. It needs the repository secrets `VERCEL_TOKEN`,
  `VERCEL_ORG_ID` and `VERCEL_PROJECT_ID`.

To keep the merge easy, the web branch adds files of its own and touches `main`'s files as little as
possible (`apps/draftcraft-web/Cargo.toml`: the viewer binary and one web-sys feature; `src/main.rs`: the
phone check). New viewer strings go in `src/viewer/text.rs` (English and Italian), not in the full
app's tables.

## Build locally

```bash
cd apps/draftcraft-web
trunk build index.html --release --dist ../../target/site-full
trunk build viewer.html --release --dist ../../target/site-viewer
```

`trunk serve viewer.html` serves the viewer alone; add `?sample` to open the sample drawing.
