//! SHX files in the user's font folders (Options ▸ Fonts): found by name, in the folder itself
//! or a folder under it, and gone again when the folder is removed. Process-wide settings, so
//! one test in its own binary. The font is compiled here from shape source written for it.

use rustdraft_fonts::{shx, ttf};

#[test]
fn shx_fonts_in_the_users_font_folders_are_found() {
    let bytes = shx::compile("*0,4,folder test\n6,2,0,0\n*65,4,UCA\n1,064,0\n").unwrap();
    let dir = std::env::temp_dir().join(format!("rustdraft-shx-folders-{}", std::process::id()));
    let sub = dir.join("shx").join("old");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(dir.join("rustdraft-top.shx"), &bytes).unwrap();
    std::fs::write(sub.join("rustdraft-deep.shx"), &bytes).unwrap();
    assert!(shx::find("rustdraft-top.shx").is_none() && shx::find("rustdraft-deep").is_none());

    let g = shx::generation();
    ttf::set_font_dirs(&[dir.to_string_lossy().to_string()]);
    assert_ne!(shx::generation(), g, "viewers draw the text again");
    assert!(shx::find("rustdraft-top.shx").is_some_and(|f| f.has_shape(65)));
    assert!(shx::find("RUSTDRAFT-DEEP").is_some(), "in a folder under the font folder");
    assert!(ttf::available("rustdraft-deep.shx"));
    // Font lists name them.
    let names = shx::installed();
    assert!(names.iter().any(|n| n == "rustdraft-top") && names.iter().any(|n| n == "rustdraft-deep"), "{names:?}");

    // A file added later is found after Look for Fonts Again.
    std::fs::write(dir.join("rustdraft-later.shx"), &bytes).unwrap();
    ttf::rescan();
    assert!(shx::find("rustdraft-later.shx").is_some());

    ttf::set_font_dirs::<&str>(&[]);
    assert!(shx::find("rustdraft-top.shx").is_none());
    assert!(!shx::installed().iter().any(|n| n.starts_with("rustdraft-")));
    let _ = std::fs::remove_dir_all(&dir);
}
