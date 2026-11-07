//! The per-user DraftCraft settings directory (logs, autosave, plotter configurations). The host
//! chooses it ([`set`], possibly to none); without a choice it is the platform's ([`platform`]).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// `None`: no choice yet; `Some(None)`: the host chose to have no settings directory.
static CHOSEN: Mutex<Option<Option<PathBuf>>> = Mutex::new(None);

/// The platform's settings directory, with `var` looking up the OS variables: `$XDG_CONFIG_HOME/draftcraft`
/// or `~/.config/draftcraft` on Linux and the BSDs, `~/Library/Application Support/DraftCraft` on
/// macOS, `%APPDATA%\DraftCraft` on Windows. An empty variable counts as unset, so a blank value
/// never means the working directory.
pub fn platform(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let set = |k: &str| var(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(target_os = "macos") {
        set("HOME").map(|h| h.join("Library/Application Support/DraftCraft"))
    } else if cfg!(windows) {
        set("APPDATA").map(|a| a.join("DraftCraft"))
    } else {
        // The XDG spec ignores a relative XDG_CONFIG_HOME.
        set("XDG_CONFIG_HOME").filter(|p| p.is_absolute()).or_else(|| set("HOME").map(|h| h.join(".config"))).map(|c| c.join("draftcraft"))
    }
}

/// The folder the settings lived in before DraftCraft 2.0, when the program was called RustDraft:
/// `dir`'s sibling under that name, when `dir` carries today's.
pub fn old_name_folder(dir: &Path) -> Option<PathBuf> {
    let old = match dir.file_name()?.to_str()? {
        "DraftCraft" => "RustDraft",
        "draftcraft" => "rustdraft",
        _ => return None,
    };
    Some(dir.with_file_name(old))
}

/// Moves the settings folder from before 2.0 (see [`old_name_folder`]) to `dir` when `dir` doesn't
/// exist yet, so settings, logs, plotters and autosaves carry over. `Ok(true)`: it was moved.
pub fn adopt_old_folder(dir: &Path) -> std::io::Result<bool> {
    let Some(old) = old_name_folder(dir) else { return Ok(false) };
    if dir.exists() || !old.is_dir() {
        return Ok(false);
    }
    std::fs::rename(&old, dir)?;
    Ok(true)
}

/// Use `dir` as the settings directory; `None` means there is none (plotter configurations are
/// neither read nor written).
pub fn set(dir: Option<PathBuf>) {
    if let Ok(mut chosen) = CHOSEN.lock() {
        *chosen = Some(dir);
    }
}

/// The settings directory: the host's choice, else the platform's.
pub fn current() -> Option<PathBuf> {
    match CHOSEN.lock().ok().and_then(|c| c.clone()) {
        Some(choice) => choice,
        None => platform(|k| std::env::var_os(k)),
    }
}

#[cfg(test)]
mod tests {
    use super::platform;
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |k| vars.iter().find(|(n, _)| *n == k).map(|(_, v)| OsString::from(v))
    }

    #[test]
    fn the_folder_from_before_two_point_oh_is_adopted() {
        use super::{adopt_old_folder, old_name_folder};
        assert_eq!(old_name_folder(&PathBuf::from("/a/DraftCraft")), Some(PathBuf::from("/a/RustDraft")));
        assert_eq!(old_name_folder(&PathBuf::from("/a/draftcraft")), Some(PathBuf::from("/a/rustdraft")));
        assert_eq!(old_name_folder(&PathBuf::from("/a/custom")), None);
        let root = std::env::temp_dir().join(format!("dc-adopt-{}", std::process::id()));
        let (old, new) = (root.join("RustDraft"), root.join("DraftCraft"));
        std::fs::create_dir_all(old.join("logs")).unwrap();
        assert!(adopt_old_folder(&new).unwrap());
        assert!(new.join("logs").is_dir() && !old.exists());
        // Once there is a folder under today's name, nothing moves.
        std::fs::create_dir_all(&old).unwrap();
        assert!(!adopt_old_folder(&new).unwrap());
        assert!(old.is_dir());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn empty_values_are_unset() {
        assert_eq!(platform(env(&[("HOME", ""), ("XDG_CONFIG_HOME", ""), ("APPDATA", "")])), None, "not the working directory");
        assert_eq!(platform(env(&[])), None);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn linux_and_bsd_follow_xdg_config_home() {
        assert_eq!(platform(env(&[("XDG_CONFIG_HOME", "/x"), ("HOME", "/home/u")])), Some(PathBuf::from("/x/draftcraft")));
        assert_eq!(platform(env(&[("HOME", "/home/u")])), Some(PathBuf::from("/home/u/.config/draftcraft")));
        // The XDG spec says a relative XDG_CONFIG_HOME is ignored.
        assert_eq!(platform(env(&[("XDG_CONFIG_HOME", "rel"), ("HOME", "/home/u")])), Some(PathBuf::from("/home/u/.config/draftcraft")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_uses_application_support() {
        assert_eq!(platform(env(&[("HOME", "/Users/u")])), Some(PathBuf::from("/Users/u/Library/Application Support/DraftCraft")));
    }

    #[cfg(windows)]
    #[test]
    fn windows_uses_appdata() {
        assert_eq!(
            platform(env(&[("APPDATA", "C:\\Users\\u\\AppData\\Roaming")])),
            Some(PathBuf::from("C:\\Users\\u\\AppData\\Roaming").join("DraftCraft"))
        );
    }
}
