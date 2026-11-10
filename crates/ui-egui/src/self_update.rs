//! DraftCraft updating itself (desktop only): Update Now in the update window downloads the new
//! release, checks it against the release's `SHA256SUMS.txt`, puts it in place of the running copy
//! and starts it once this one has quit.
//!
//! | Installed as | Download | Put in place |
//! |---|---|---|
//! | macOS `DraftCraft.app` (bundle id `dev.skryx.draftcraft`) | `draftcraft-macos-arm64.dmg` | the new app copied out of the disk image beside the old one, then swapped in by renaming (fine while it runs) |
//! | Windows folder with `DraftCraft.exe` | `draftcraft-windows-{x64,arm64}.zip` | unzipped beside it; each running file renamed to `*.old` (Windows allows that) and the new one moved in |
//! | Linux AppImage (`$APPIMAGE`) | `draftcraft-linux-x86_64.AppImage` | renamed over the old one (fine while it runs) |
//!
//! Anything else (a build from source, a folder this account can't write, another system) gets the
//! release page instead, as before. The leftovers of a swap (the old app, `*.old` files) are deleted
//! at the next start ([`clean_leftovers`]).
//!
//! Downloading, resuming and the checksum are `craft_update::Http::download`; only files under
//! `https://github.com/SkryxDev/DraftCraft/releases/download/` are fetched.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use craft_update::http::{Stop, Transfer};
use craft_update::{Asset, Http, Release};

/// macOS bundle identifier of DraftCraft.app (`apps/draftcraft/Info.plist.in`).
const BUNDLE_ID: &str = "dev.skryx.draftcraft";

/// The largest download accepted (a release file is about 75 MB).
const MAX_DOWNLOAD: u64 = 600 * 1024 * 1024;

/// The most files and bytes a Windows update zip may unpack to.
const MAX_ZIP_ENTRIES: usize = 500;
const MAX_ZIP_BYTES: u64 = 1024 * 1024 * 1024;

/// How deep the folders of a Windows update may nest.
const MAX_DEPTH: usize = 8;

/// Names beside the running copy that a swap leaves behind.
const MAC_STAGED: &str = ".DraftCraft-update.app";
const MAC_OLD: &str = ".DraftCraft-old.app";
const WIN_STAGING: &str = ".draftcraft-update";
const OLD_SUFFIX: &str = ".old";
const LINUX_STAGED: &str = ".draftcraft-update.AppImage";

/// How the running copy was installed, when it can replace itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Install {
    /// macOS: the `DraftCraft.app` bundle.
    MacApp(PathBuf),
    /// Windows: the folder holding `DraftCraft.exe`.
    WindowsFolder(PathBuf),
    /// Linux: the AppImage file.
    AppImage(PathBuf),
}

impl Install {
    /// The release file this install updates from, on this processor.
    pub fn asset_name(&self) -> Option<&'static str> {
        match self {
            Self::MacApp(_) => cfg!(target_arch = "aarch64").then_some("draftcraft-macos-arm64.dmg"),
            Self::WindowsFolder(_) => Some(if cfg!(target_arch = "aarch64") { "draftcraft-windows-arm64.zip" } else { "draftcraft-windows-x64.zip" }),
            Self::AppImage(_) => cfg!(target_arch = "x86_64").then_some("draftcraft-linux-x86_64.AppImage"),
        }
    }

    /// What to start after the update.
    fn program(&self) -> PathBuf {
        match self {
            Self::MacApp(bundle) => bundle.clone(),
            Self::WindowsFolder(dir) => dir.join("DraftCraft.exe"),
            Self::AppImage(path) => path.clone(),
        }
    }
}

/// How this running copy can update itself; `None` when it can't (the release page is offered).
pub fn detect() -> Option<Install> {
    let exe = std::env::current_exe().ok()?;
    let exe = exe.canonicalize().unwrap_or(exe);
    if is_development_build(&exe) {
        return None;
    }
    let install = detect_at(&exe, std::env::var_os("APPIMAGE").map(PathBuf::from))?;
    install.asset_name()?;
    writable(container(&install)?).then_some(install)
}

/// [`detect`] for a known executable and `$APPIMAGE`, without the writability check.
fn detect_at(exe: &Path, appimage: Option<PathBuf>) -> Option<Install> {
    if cfg!(target_os = "macos") {
        let bundle = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app"))?;
        return (bundle_value(bundle, "CFBundleIdentifier").as_deref() == Some(BUNDLE_ID)).then(|| Install::MacApp(bundle.to_path_buf()));
    }
    if cfg!(windows) {
        let named = exe.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case("DraftCraft.exe"));
        return named.then(|| exe.parent().map(|d| Install::WindowsFolder(d.to_path_buf()))).flatten();
    }
    if cfg!(target_os = "linux") {
        return appimage.filter(|p| p.is_file()).map(Install::AppImage);
    }
    None
}

/// The folder the swap writes in.
fn container(install: &Install) -> Option<&Path> {
    match install {
        Install::MacApp(bundle) | Install::AppImage(bundle) => bundle.parent(),
        Install::WindowsFolder(dir) => Some(dir),
    }
}

/// Whether this account can create files in `dir`.
fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".draftcraft-write-test-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Whether `exe` runs from a Cargo build folder (a build from source updates through git).
fn is_development_build(exe: &Path) -> bool {
    exe.ancestors().skip(1).any(|d| d.join(".rustc_info.json").is_file() || d.join("CACHEDIR.TAG").is_file())
}

/// A value from a macOS bundle's Info.plist, read with `plutil`.
fn bundle_value(bundle: &Path, key: &str) -> Option<String> {
    let plist = bundle.join("Contents/Info.plist");
    let out = std::process::Command::new("/usr/bin/plutil").args(["-extract", key, "raw", "-o", "-"]).arg(&plist).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// What an update reports while it works.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    /// Bytes downloaded of the total.
    Downloading { done: u64, total: u64 },
    /// The connection dropped; trying again.
    Reconnecting { done: u64, total: u64 },
    /// Downloaded and checked; putting it in place.
    Installing,
}

/// The release file `install` updates from, checked: one of this repository's downloads, with a
/// size and a SHA-256.
fn pick(release: &Release, install: &Install) -> Result<Asset, String> {
    let name = install.asset_name().ok_or("this release has no build for this computer")?;
    let asset = release.assets.iter().find(|a| a.name == name).ok_or_else(|| format!("the release has no {name}"))?;
    if !craft_update::release::download_url_under(&asset.url, &craft_update::release::download_prefix(crate::UPDATE_APP.repo)) {
        return Err(format!("{name} isn't one of DraftCraft's own downloads"));
    }
    if asset.size == 0 || asset.size > MAX_DOWNLOAD {
        return Err(format!("{name} has an unexpected size ({} bytes)", asset.size));
    }
    if !asset.sha256.as_deref().is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())) {
        return Err(format!("the release publishes no checksum for {name}"));
    }
    Ok(asset.clone())
}

/// Fill in the release's checksums from its `SHA256SUMS.txt`, when GitHub didn't give them.
fn add_checksums(http: &Http, release: &mut Release) -> Result<(), String> {
    if release.assets.iter().all(|a| a.sha256.is_some()) {
        return Ok(());
    }
    let Some(sums) = release.sums_asset() else { return Ok(()) };
    if !craft_update::release::download_url_under(&sums.url, &craft_update::release::download_prefix(crate::UPDATE_APP.repo)) {
        return Err("the release's checksum file isn't one of DraftCraft's own downloads".into());
    }
    let text = http.get_text(&sums.url)?.ok_or("the release's checksum file is missing")?;
    release.apply_sums(&craft_update::release::parse_sha256sums(&text));
    Ok(())
}

/// Where downloads wait until they are installed.
fn downloads_dir() -> PathBuf {
    draftcraft_engine::settings_dir::current().map_or_else(|| std::env::temp_dir().join("draftcraft-updates"), |d| d.join("updates"))
}

/// Download `release` for `install`, check it and put it in place. Returns what to start after
/// quitting. Runs on its own thread; `stop` cancels the download.
pub fn install(release: &Release, install: &Install, stop: &Stop, progress: &mut dyn FnMut(Progress)) -> Result<PathBuf, String> {
    let version = release.version_label();
    let http = Http::new(&format!("DraftCraft/{}", env!("CARGO_PKG_VERSION")))?;
    let mut release = release.clone();
    add_checksums(&http, &mut release)?;
    let asset = pick(&release, install)?;
    let sha = asset.sha256.clone().unwrap_or_default();
    let dir = downloads_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't create {} ({e})", dir.display()))?;
    let file = dir.join(&asset.name);
    http.download(&asset.url, &file, asset.size, &sha, stop, &mut |t| {
        progress(match t {
            Transfer::Bytes { done, total } => Progress::Downloading { done, total },
            Transfer::Reconnecting { done, total } => Progress::Reconnecting { done, total },
        });
    })?;
    progress(Progress::Installing);
    let placed = match install {
        Install::MacApp(bundle) => install_mac(&file, bundle, &version),
        Install::WindowsFolder(folder) => install_windows(&file, folder),
        Install::AppImage(path) => install_appimage(&file, path),
    };
    let _ = std::fs::remove_file(&file);
    placed?;
    log::info!("DraftCraft {version} installed at {}", install.program().display());
    Ok(install.program())
}

/// macOS: mount the disk image, copy its DraftCraft.app beside `bundle`, check it, swap it in.
fn install_mac(dmg: &Path, bundle: &Path, version: &str) -> Result<(), String> {
    let parent = bundle.parent().ok_or("DraftCraft.app isn't in a folder")?;
    let mount = downloads_dir().join("mount");
    let _ = std::fs::create_dir_all(&mount);
    run(
        "/usr/bin/hdiutil",
        &[
            "attach".as_ref(),
            "-readonly".as_ref(),
            "-nobrowse".as_ref(),
            "-noautoopen".as_ref(),
            "-mountpoint".as_ref(),
            mount.as_os_str(),
            dmg.as_os_str(),
        ],
    )?;
    let staged = parent.join(MAC_STAGED);
    let copied = (|| {
        let new = mount.join("DraftCraft.app");
        if bundle_value(&new, "CFBundleIdentifier").as_deref() != Some(BUNDLE_ID) {
            return Err("the disk image doesn't hold DraftCraft".to_string());
        }
        let found = bundle_value(&new, "CFBundleShortVersionString").unwrap_or_default();
        if found != version {
            return Err(format!("the disk image holds DraftCraft {found}, not {version}"));
        }
        let _ = std::fs::remove_dir_all(&staged);
        run("/usr/bin/ditto", &[new.as_os_str(), staged.as_os_str()])
    })();
    let _ = run("/usr/bin/hdiutil", &["detach".as_ref(), "-force".as_ref(), mount.as_os_str()]);
    if let Err(e) = copied {
        let _ = std::fs::remove_dir_all(&staged);
        return Err(e);
    }
    swap_in(&staged, bundle, &parent.join(MAC_OLD))
}

/// Put `new` at `target`, moving what is there to `old` first, and back if the move fails.
fn swap_in(new: &Path, target: &Path, old: &Path) -> Result<(), String> {
    remove_any(old);
    if target.exists() {
        std::fs::rename(target, old).map_err(|e| format!("couldn't move the old {} aside ({e})", target.display()))?;
    }
    if let Err(e) = std::fs::rename(new, target) {
        let _ = std::fs::rename(old, target);
        return Err(format!("couldn't put the new {} in place ({e})", target.display()));
    }
    Ok(())
}

fn remove_any(path: &Path) {
    if path.is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else {
        let _ = std::fs::remove_file(path);
    }
}

/// Windows: unzip beside the running copy, then move each file in, renaming the one it replaces
/// to `*.old` (a running program can be renamed, not overwritten).
fn install_windows(zip: &Path, folder: &Path) -> Result<(), String> {
    let staging = folder.join(WIN_STAGING);
    remove_any(&staging);
    let done = unzip(zip, &staging).and_then(|()| {
        if !staging.join("DraftCraft.exe").is_file() {
            return Err("the download doesn't hold DraftCraft.exe".to_string());
        }
        move_in(&staging, folder, 0)
    });
    remove_any(&staging);
    done
}

/// Unpack `zip` into `dest`, dropping the archive's top `DraftCraft/` folder. Entries that would
/// land outside `dest`, too many entries or too many bytes are refused.
fn unzip(zip: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip).map_err(|e| format!("couldn't open the download ({e})"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("the download isn't a zip ({e})"))?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err("the download holds too many files".into());
    }
    let mut total = 0u64;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("unreadable zip entry ({e})"))?;
        let Some(name) = entry.enclosed_name() else { return Err("a file in the download would land outside its folder".to_string()) };
        let rel: PathBuf = name.components().skip_while(|c| c.as_os_str().eq_ignore_ascii_case("DraftCraft")).take(MAX_DEPTH + 1).collect();
        if rel.as_os_str().is_empty() {
            continue;
        }
        if rel.components().count() > MAX_DEPTH {
            return Err("the download's folders nest too deep".into());
        }
        let out = dest.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| format!("couldn't create {} ({e})", out.display()))?;
            continue;
        }
        total = total.saturating_add(entry.size());
        if total > MAX_ZIP_BYTES {
            return Err("the download unpacks to too much".into());
        }
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("couldn't create {} ({e})", dir.display()))?;
        }
        let mut f = std::fs::File::create(&out).map_err(|e| format!("couldn't write {} ({e})", out.display()))?;
        let mut limited = std::io::Read::take(&mut entry, MAX_ZIP_BYTES);
        std::io::copy(&mut limited, &mut f).map_err(|e| format!("couldn't unpack {} ({e})", rel.display()))?;
    }
    Ok(())
}

/// Move every file of `from` into `to` (same folders), renaming each file it replaces to
/// `<name>.old` (or `<name>.old2`, … when an earlier one is still in use).
fn move_in(from: &Path, to: &Path, depth: usize) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("the download's folders nest too deep".into());
    }
    let entries = std::fs::read_dir(from).map_err(|e| format!("couldn't read {} ({e})", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("couldn't read {} ({e})", from.display()))?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            std::fs::create_dir_all(&dst).map_err(|e| format!("couldn't create {} ({e})", dst.display()))?;
            move_in(&src, &dst, depth + 1)?;
            continue;
        }
        if dst.exists() {
            let aside = old_name(&dst).ok_or_else(|| format!("couldn't move the old {} aside", dst.display()))?;
            std::fs::rename(&dst, &aside).map_err(|e| format!("couldn't move the old {} aside ({e})", dst.display()))?;
        }
        std::fs::rename(&src, &dst).map_err(|e| format!("couldn't put the new {} in place ({e})", dst.display()))?;
    }
    Ok(())
}

/// A free `<file>.old` name for `file`, deleting an earlier one when it can.
fn old_name(file: &Path) -> Option<PathBuf> {
    let name = file.file_name()?.to_str()?;
    for n in 1..=20u32 {
        let suffix = if n == 1 { OLD_SUFFIX.to_string() } else { format!("{OLD_SUFFIX}{n}") };
        let candidate = file.with_file_name(format!("{name}{suffix}"));
        if !candidate.exists() || std::fs::remove_file(&candidate).is_ok() {
            return Some(candidate);
        }
    }
    None
}

/// Linux: copy the new AppImage beside the old one and rename it over.
fn install_appimage(file: &Path, appimage: &Path) -> Result<(), String> {
    let staged = appimage.with_file_name(LINUX_STAGED);
    std::fs::copy(file, &staged).map_err(|e| format!("couldn't copy the new AppImage ({e})"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)) {
            let _ = std::fs::remove_file(&staged);
            return Err(format!("couldn't make the new AppImage executable ({e})"));
        }
    }
    std::fs::rename(&staged, appimage).map_err(|e| {
        let _ = std::fs::remove_file(&staged);
        format!("couldn't put the new AppImage in place ({e})")
    })
}

/// Run a system tool; its error output on failure.
fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let out = std::process::Command::new(program).args(args).output().map_err(|e| format!("couldn't run {program} ({e})"))?;
    if out.status.success() { Ok(()) } else { Err(format!("{program} failed: {}", String::from_utf8_lossy(&out.stderr).trim())) }
}

/// The program to start once this one has quit.
static RELAUNCH: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Start `program` after this run of DraftCraft ends ([`relaunch_if_scheduled`]); `None` cancels.
pub fn relaunch_after_exit(program: Option<PathBuf>) {
    if let Ok(mut slot) = RELAUNCH.lock() {
        *slot = program;
    }
}

/// Called by the app once its window has closed: start the updated DraftCraft, if an update asked.
pub fn relaunch_if_scheduled() {
    let Some(program) = RELAUNCH.lock().ok().and_then(|mut slot| slot.take()) else { return };
    let mut cmd = if cfg!(target_os = "macos") {
        let mut open = std::process::Command::new("/usr/bin/open");
        open.arg("-n").arg(&program);
        open
    } else {
        std::process::Command::new(&program)
    };
    // A pretend version (for trying the update) lasts one run.
    cmd.env_remove("DRAFTCRAFT_PRETEND_VERSION");
    if let Err(e) = cmd.spawn() {
        log::warn!("couldn't start the updated DraftCraft ({}): {e}", program.display());
    }
}

/// At start: delete what an earlier update left beside this copy, and its downloads.
pub fn clean_leftovers() {
    let Some(exe) = std::env::current_exe().ok().map(|e| e.canonicalize().unwrap_or(e)) else { return };
    if let Some(install) = detect_at(&exe, std::env::var_os("APPIMAGE").map(PathBuf::from)) {
        clean_beside(&install);
    }
    remove_any(&downloads_dir().join("mount"));
}

fn clean_beside(install: &Install) {
    match install {
        Install::MacApp(bundle) => {
            if let Some(parent) = bundle.parent() {
                remove_any(&parent.join(MAC_OLD));
                remove_any(&parent.join(MAC_STAGED));
            }
        }
        Install::WindowsFolder(dir) => {
            remove_any(&dir.join(WIN_STAGING));
            remove_old_files(dir, 0);
        }
        Install::AppImage(path) => remove_any(&path.with_file_name(LINUX_STAGED)),
    }
}

/// Delete the `*.old`, `*.old2`, … files a Windows update renamed.
fn remove_old_files(dir: &Path, depth: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < MAX_DEPTH {
                remove_old_files(&path, depth + 1);
            }
        } else if is_old_name(&path) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn is_old_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { return false };
    let Some(pos) = name.rfind(OLD_SUFFIX) else { return false };
    pos > 0 && name.get(pos + OLD_SUFFIX.len()..).is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(assets: &[(&str, &str, u64, Option<&str>)]) -> Release {
        Release {
            tag: "v9.0.0".into(),
            version: craft_update::parse_version("9.0.0"),
            assets: assets
                .iter()
                .map(|(name, url, size, sha)| Asset { name: (*name).into(), url: (*url).into(), size: *size, sha256: sha.map(Into::into) })
                .collect(),
            ..Release::default()
        }
    }

    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn ours(name: &str) -> String {
        format!("https://github.com/SkryxDev/DraftCraft/releases/download/v9.0.0/{name}")
    }

    #[test]
    fn only_our_checked_downloads_are_picked() {
        let install = Install::WindowsFolder(PathBuf::from("C:/DraftCraft"));
        let name = install.asset_name().unwrap();
        let url = ours(name);
        assert_eq!(pick(&release(&[(name, &url, 10, Some(SHA))]), &install).unwrap().name, name);
        // Another repository, a path that climbs out, no checksum, no size, too big, missing.
        for (u, size, sha) in [
            ("https://github.com/someone/else/releases/download/v9.0.0/x.zip", 10, Some(SHA)),
            ("https://github.com/SkryxDev/DraftCraft/releases/download/../../../x/y/releases/download/v1/z", 10, Some(SHA)),
            (url.as_str(), 10, None),
            (url.as_str(), 10, Some("abc")),
            (url.as_str(), 0, Some(SHA)),
            (url.as_str(), MAX_DOWNLOAD + 1, Some(SHA)),
        ] {
            assert!(pick(&release(&[(name, u, size, sha)]), &install).is_err(), "{u} {size} {sha:?}");
        }
        assert!(pick(&release(&[]), &install).is_err());
    }

    #[test]
    fn each_install_updates_from_its_own_file() {
        let win = Install::WindowsFolder(PathBuf::from("C:/DraftCraft")).asset_name().unwrap();
        assert!(win == "draftcraft-windows-x64.zip" || win == "draftcraft-windows-arm64.zip");
        let mac = Install::MacApp(PathBuf::from("/Applications/DraftCraft.app"));
        assert_eq!(mac.asset_name().is_some(), cfg!(target_arch = "aarch64"));
        assert_eq!(mac.program(), PathBuf::from("/Applications/DraftCraft.app"));
        let linux = Install::AppImage(PathBuf::from("/home/me/DraftCraft.AppImage"));
        assert_eq!(linux.asset_name().is_some(), cfg!(target_arch = "x86_64"));
    }

    #[test]
    fn a_cargo_build_is_a_development_build() {
        let dir = tempdir();
        std::fs::create_dir_all(dir.join("target/release")).unwrap();
        std::fs::write(dir.join("target/.rustc_info.json"), "{}").unwrap();
        assert!(is_development_build(&dir.join("target/release/draftcraft")));
        assert!(!is_development_build(&dir.join("DraftCraft/DraftCraft.exe")));
    }

    #[test]
    fn swap_in_moves_the_old_copy_aside_and_back_on_failure() {
        let dir = tempdir();
        let (new, target, old) = (dir.join("new.app"), dir.join("DraftCraft.app"), dir.join("old.app"));
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("v"), "2").unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("v"), "1").unwrap();
        swap_in(&new, &target, &old).unwrap();
        assert_eq!(std::fs::read_to_string(target.join("v")).unwrap(), "2");
        assert_eq!(std::fs::read_to_string(old.join("v")).unwrap(), "1");
        // Nothing new to move in: the old copy comes back.
        let missing = dir.join("missing.app");
        assert!(swap_in(&missing, &target, &old).is_err());
        assert_eq!(std::fs::read_to_string(target.join("v")).unwrap(), "2");
    }

    fn write_zip(path: &Path, files: &[(&str, &[u8])]) {
        let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, bytes) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut w, bytes).unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn a_windows_update_replaces_files_and_keeps_the_old_ones_aside() {
        let dir = tempdir();
        let folder = dir.join("DraftCraft");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("DraftCraft.exe"), "old").unwrap();
        std::fs::write(folder.join("settings.txt"), "mine").unwrap();
        let zip = dir.join("draftcraft-windows-x64.zip");
        write_zip(&zip, &[("DraftCraft/DraftCraft.exe", b"new"), ("DraftCraft/draftcraft-cli.exe", b"cli"), ("DraftCraft/docs/NOTICE", b"n")]);
        install_windows(&zip, &folder).unwrap();
        assert_eq!(std::fs::read_to_string(folder.join("DraftCraft.exe")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(folder.join("DraftCraft.exe.old")).unwrap(), "old");
        assert_eq!(std::fs::read_to_string(folder.join("draftcraft-cli.exe")).unwrap(), "cli");
        assert_eq!(std::fs::read_to_string(folder.join("docs/NOTICE")).unwrap(), "n");
        assert_eq!(std::fs::read_to_string(folder.join("settings.txt")).unwrap(), "mine");
        assert!(!folder.join(WIN_STAGING).exists());
        // The next start deletes what the swap left.
        clean_beside(&Install::WindowsFolder(folder.clone()));
        assert!(!folder.join("DraftCraft.exe.old").exists());
        assert!(folder.join("DraftCraft.exe").exists() && folder.join("settings.txt").exists());
    }

    #[test]
    fn a_hostile_zip_is_refused() {
        let dir = tempdir();
        let folder = dir.join("DraftCraft");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("DraftCraft.exe"), "old").unwrap();
        let zip = dir.join("evil.zip");
        write_zip(&zip, &[("../../outside.txt", b"x"), ("DraftCraft/DraftCraft.exe", b"new")]);
        assert!(install_windows(&zip, &folder).is_err());
        assert!(!dir.join("outside.txt").exists());
        assert_eq!(std::fs::read_to_string(folder.join("DraftCraft.exe")).unwrap(), "old");
        // A zip without the program changes nothing.
        write_zip(&zip, &[("DraftCraft/readme.txt", b"x")]);
        assert!(install_windows(&zip, &folder).is_err());
        assert!(!folder.join("readme.txt").exists());
    }

    #[test]
    fn old_names_are_recognised() {
        for (name, old) in [("DraftCraft.exe.old", true), ("a.old2", true), ("a.old12", true), ("a.older", false), (".old", false), ("a.txt", false)]
        {
            assert_eq!(is_old_name(Path::new(name)), old, "{name}");
        }
    }

    #[test]
    fn an_appimage_is_replaced_in_place() {
        let dir = tempdir();
        let (app, new) = (dir.join("DraftCraft.AppImage"), dir.join("download"));
        std::fs::write(&app, "old").unwrap();
        std::fs::write(&new, "new").unwrap();
        install_appimage(&new, &app).unwrap();
        assert_eq!(std::fs::read_to_string(&app).unwrap(), "new");
        assert!(!dir.join(LINUX_STAGED).exists());
    }

    #[test]
    fn a_relaunch_can_be_cancelled() {
        relaunch_after_exit(Some(PathBuf::from("/nonexistent/DraftCraft")));
        relaunch_after_exit(None);
        assert!(RELAUNCH.lock().unwrap().is_none());
    }

    fn tempdir() -> PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("draftcraft-self-update-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
