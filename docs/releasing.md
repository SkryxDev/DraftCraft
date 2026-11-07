# Releasing RustDraft

## 1. Set the version

`[workspace.package] version` in the root `Cargo.toml` is the single source of the version. The app
shows it in About, `rustdraft --version` and `rustdraft-cli --version`.

```bash
cargo xtask version            # prints it
cargo xtask version set 1.1.0  # changes it
cargo xtask ci                 # the quality checks
git commit -am "RustDraft 1.1.0" && git push origin main
```

## 2. Tag: every package is built for you

```bash
git tag -a v1.1.0 -m "RustDraft 1.1.0"
git push origin v1.1.0
```

Pushing a `v*` tag runs [`.github/workflows/release.yml`](../.github/workflows/release.yml). It builds

| Package | Contents |
| --- | --- |
| `RustDraft-<version>-macos-arm64.dmg` | `RustDraft.app` (Apple silicon, with `rustdraft-cli` inside) |
| `rustdraft-cli-<version>-macos-arm64.zip` | `rustdraft-cli` for macOS on its own |
| `RustDraft-<version>-windows-x86_64.zip` | `RustDraft.exe`, `rustdraft-cli.exe`, licences |
| `RustDraft-<version>-windows-arm64.zip` | the same for Windows on ARM (cross-compiled) |
| `RustDraft-<version>-linux-x86_64.tar.gz` | `rustdraft`, `rustdraft-cli`, desktop entry, icons, licences |

and creates the GitHub Release `RustDraft v<version>` with them and `SHA256SUMS.txt`. The DMG is
made on a macOS 26 runner, whose Xcode compiles the app icon. The workflow can also be started by hand
(**Actions › Release › Run workflow**) for an existing tag.

## 3. A macOS build on your own Mac

The release doesn't need it, but the same script makes the DMG locally (Apple-silicon Mac with Xcode):

```bash
git clone https://github.com/storytold/craft-fonts.git ../craft-fonts   # once
CRAFT_FONTS_DIR="$PWD/../craft-fonts" packaging/macos/package.sh --arch aarch64
```

It writes `dist/release/rustdraft-<version>-macos-aarch64.dmg` and
`rustdraft-cli-<version>-macos-aarch64.zip`. `--arch universal` also builds for Intel Macs (it needs
`rustup target add x86_64-apple-darwin`).

## Signing

The builds are not signed with a paid certificate: macOS builds are signed ad hoc and Windows builds
are not signed, so users see the warnings the README explains. `packaging/macos/package.sh` signs and
notarizes with a Developer ID when `MACOS_SIGN_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` and
`APPLE_TEAM_ID` are set (see the top of the script).

## Release notes

Edit them on GitHub or with `gh release edit v1.1.0 --notes-file notes.md`: what is new, what was fixed,
and the download table.
