# Releasing DraftCraft

## 1. Set the version

`[workspace.package] version` in the root `Cargo.toml` is the single source of the version. The app
shows it in About, `draftcraft --version` and `draftcraft-cli --version`.

```bash
cargo xtask version            # prints it
cargo xtask version set 1.1.0  # changes it
cargo xtask ci                 # the quality checks
git commit -am "DraftCraft 1.1.0" && git push origin main
```

## 2. Tag: every package is built for you

```bash
git tag -a v1.1.0 -m "DraftCraft 1.1.0"
git push origin v1.1.0
```

Pushing a `v*` tag runs [`.github/workflows/release.yml`](../.github/workflows/release.yml). It builds

| Package | Contents |
| --- | --- |
| `draftcraft-macos-arm64.dmg` | `DraftCraft.app` (Apple silicon, with `draftcraft-cli` inside) |
| `draftcraft-cli-macos-arm64.zip` | `draftcraft-cli` for macOS on its own |
| `draftcraft-windows-x64.zip` | `DraftCraft.exe`, `draftcraft-cli.exe`, licences |
| `draftcraft-windows-arm64.zip` | the same for Windows on ARM (cross-compiled) |
| `draftcraft-linux-x86_64.AppImage` | the app; `… cli <args>` runs `draftcraft-cli` (built by `packaging/linux/appimage.sh`) |

The names carry no version, so the README's `releases/latest/download/<name>` links always give the
newest. The workflow creates the GitHub Release `DraftCraft v<version>` with them and `SHA256SUMS.txt`. The DMG is
made on a macOS 26 runner, whose Xcode compiles the app icon. The workflow can also be started by hand
(**Actions › Release › Run workflow**) for an existing tag.

### In-app updates depend on these names

From 2.0.2, **Update Now** in the app's update window downloads the new release and installs it
([`crates/ui-egui/src/self_update.rs`](../crates/ui-egui/src/self_update.rs)): `DraftCraft.app` from
`draftcraft-macos-arm64.dmg`, a Windows folder from `draftcraft-windows-{x64,arm64}.zip` (whose top
folder is `DraftCraft/`), the AppImage from `draftcraft-linux-x86_64.AppImage`. Each file is checked
against `SHA256SUMS.txt`, and the DMG's app must have the bundle id `dev.skryx.draftcraft` and the
release's version. Keep those names, that folder and `SHA256SUMS.txt`, or installed copies can no
longer update themselves (they then offer the release page instead).

## 3. A macOS build on your own Mac

The release doesn't need it, but the same script makes the DMG locally (Apple-silicon Mac with Xcode):

```bash
git clone https://github.com/storytold/craft-fonts.git ../craft-fonts   # once
CRAFT_FONTS_DIR="$PWD/../craft-fonts" packaging/macos/package.sh --arch aarch64
```

It writes `dist/release/draftcraft-<version>-macos-aarch64.dmg` and
`draftcraft-cli-<version>-macos-aarch64.zip`. `--arch universal` also builds for Intel Macs (it needs
`rustup target add x86_64-apple-darwin`).

## Signing

The builds are not signed with a paid certificate: macOS builds are signed ad hoc and Windows builds
are not signed, so users see the warnings the README explains. `packaging/macos/package.sh` signs and
notarizes with a Developer ID when `MACOS_SIGN_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` and
`APPLE_TEAM_ID` are set (see the top of the script).

## Release notes

Edit them on GitHub or with `gh release edit v1.1.0 --notes-file notes.md`: what is new, what was fixed,
and the download table.
