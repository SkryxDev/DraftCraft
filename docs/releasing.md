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

Pushing a `v*` tag runs [`.github/workflows/release.yml`](../.github/workflows/release.yml), which
builds every package with [`packages.yml`](../.github/workflows/packages.yml):

| Package | Contents |
| --- | --- |
| `draftcraft-macos-arm64.dmg` | `DraftCraft.app` (Apple silicon, with `draftcraft-cli` inside) |
| `draftcraft-cli-macos-arm64.zip` | `draftcraft-cli` for macOS on its own |
| `draftcraft-windows-x64.msi` | the installer (`packaging/windows/package.ps1`, WiX 5): for this account or everyone, Start menu, optional desktop shortcut, `.dxf`/`.dwg` registration |
| `draftcraft-windows-arm64.msi` | the same for Windows on ARM (cross-compiled) |
| `draftcraft-windows-x64.zip` | `DraftCraft.exe`, `draftcraft-cli.exe`, licences, to unzip anywhere |
| `draftcraft-windows-arm64.zip` | the same for Windows on ARM |
| `draftcraft-linux-x86_64.AppImage` | the app; `… cli <args>` runs `draftcraft-cli` (built by `packaging/linux/appimage.sh`) |
| `draftcraft-linux-x86_64.deb` | Debian/Ubuntu package (`packaging/linux/package.sh`, nfpm): `/usr/bin`, menu entry, icons, MIME types, man pages |

The names carry no version, so the README's `releases/latest/download/<name>` links always give the
newest. The same workflow runs on every pull request that changes `packaging/` or the workflows; its
packages are then that run's artifacts, so a packaging change can be tried before a release. On the
way it installs the x64 MSI and the `.deb` on the runner, checks the `.dxf`/`.dwg` registration, and
uninstalls them. The workflow creates the GitHub Release `DraftCraft v<version>` with them and `SHA256SUMS.txt`. The DMG is
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

A copy installed by the MSI (its `draftcraft.exe` is the one named under `App Paths`) or by the
`.deb` offers the release page too: the new installer or package replaces it, keeping Windows'
and the package manager's record of the installed version right.

### Opening `.dxf` and `.dwg` files

| System | How DraftCraft is registered |
| --- | --- |
| Windows (MSI) | `DraftCraft.Drawing` ProgId, `OpenWithProgids` of `.dxf`/`.dwg`, `Applications\draftcraft.exe\SupportedTypes`, and `Capabilities` in `RegisteredApplications` (`packaging/windows/draftcraft.wxs`), in the install's own context. It appears in **Open with** and in **Settings › Apps › Default apps**; Windows lets only the user make it the default. The zip registers nothing. |
| macOS | `CFBundleDocumentTypes` and `UTImportedTypeDeclarations` in `packaging/macos/Info.plist.in`, with `LSHandlerRank` `Alternate`: Finder lists it in **Open With**, and **Get Info › Open with › Change All…** makes it the default. |
| Linux (`.deb`) | `MimeType=` in `dev.skryx.draftcraft.desktop` and the `image/vnd.dxf`/`image/vnd.dwg` types in `dev.skryx.draftcraft.mime.xml`; the package's scripts refresh the desktop, MIME and icon caches. The AppImage registers nothing. |

The app opens the files it is given on its command line (`draftcraft [files…]`), which is how Windows
and Linux hand them over. macOS hands them over as an Apple event instead, which the app does not
handle yet, so on macOS a file opened from Finder starts DraftCraft without loading it.

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
are not signed, so users see the warnings the README explains. Both are ready for certificates; adding
these repository secrets is enough, no workflow change:

| Secret | Effect |
| --- | --- |
| `APPLE_CERTIFICATE` (base64 Developer ID Application `.p12`), `APPLE_CERTIFICATE_PASSWORD` | `packaging/macos/import-cert.sh` imports it, and `package.sh` signs the app, CLI and DMG with it (hardened runtime, `entitlements.plist`) |
| `APPLE_ID`, `APPLE_PASSWORD` (app-specific password), `APPLE_TEAM_ID` | with the certificate, `package.sh` notarizes and staples the app and the DMG |
| `WINDOWS_CERTIFICATE` (base64 `.pfx`), `WINDOWS_CERTIFICATE_PASSWORD` | `packaging/windows/sign.ps1` signs the exes and the MSI |

Locally, `package.sh` reads `MACOS_SIGN_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID`
(see the top of the script).

## Release notes

Edit them on GitHub or with `gh release edit v1.1.0 --notes-file notes.md`: what is new, what was fixed,
and the download table.
