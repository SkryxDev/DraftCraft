//! Drawings saved before DraftCraft 2.0, when the program was called RustDraft, name DraftCraft's own
//! data with that name: the extended-data application (`RUSTDRAFT`), its dictionary entries and
//! header variables (`RUSTDRAFT_…`), the built-in drafting font and the built-in plotters. Right after
//! a DXF (or a DWG, through its DXF) is parsed, those names are changed to today's, so such a
//! drawing opens with everything DraftCraft stored in it.

use draftcraft_dxf::{Tag, Value};

/// The program's name before 2.0, in the spellings its files used.
const OLD_APP: &str = "RUSTDRAFT";
const OLD_NAME: &str = "RustDraft";

/// Names written as "RustDraft <name>": the built-in font and plotters.
const OLD_NAMED: &[&str] = &["Stroke", "PDF", "PNG", "SVG"];

/// Change every old name in `tags` to today's.
pub(crate) fn upgrade(tags: &mut [Tag]) {
    for t in tags {
        if let Value::Str(s) = &mut t.value
            && let Some(new) = upgraded(s)
        {
            *s = new;
        }
    }
}

/// Today's name for `s`, when it is one of the old ones.
fn upgraded(s: &str) -> Option<String> {
    if s == OLD_APP {
        return Some("DRAFTCRAFT".to_string());
    }
    for prefix in ["", "$"] {
        if let Some(rest) = s.strip_prefix(prefix).and_then(|r| r.strip_prefix(OLD_APP)).and_then(|r| r.strip_prefix('_')) {
            return Some(format!("{prefix}DRAFTCRAFT_{rest}"));
        }
    }
    let rest = s.strip_prefix(OLD_NAME)?.strip_prefix(' ')?;
    OLD_NAMED.contains(&rest).then(|| format!("DraftCraft {rest}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_names_become_new_ones() {
        assert_eq!(upgraded("RUSTDRAFT").as_deref(), Some("DRAFTCRAFT"));
        assert_eq!(upgraded("RUSTDRAFT_CONSTRAINTS").as_deref(), Some("DRAFTCRAFT_CONSTRAINTS"));
        assert_eq!(upgraded("$RUSTDRAFT_LAYERP").as_deref(), Some("$DRAFTCRAFT_LAYERP"));
        assert_eq!(upgraded("RustDraft Stroke").as_deref(), Some("DraftCraft Stroke"));
        assert_eq!(upgraded("RustDraft PDF").as_deref(), Some("DraftCraft PDF"));
        for kept in ["RUSTDRAFTX", "RustDraft", "RustDraft notes", "Wall", "", "DRAFTCRAFT"] {
            assert_eq!(upgraded(kept), None, "{kept}");
        }
    }

    /// A drawing saved by RustDraft 1.x: its constraints dictionary, xdata and header variable
    /// come back under today's names.
    #[test]
    fn a_rustdraft_file_reads_its_own_data() {
        let mut tags = vec![
            Tag::s(9, "$RUSTDRAFT_LAYERP"),
            Tag::s(1, "{}"),
            Tag::s(3, "RUSTDRAFT_CONSTRAINTS"),
            Tag::s(1001, "RUSTDRAFT"),
            Tag::s(7, "Standard"),
            Tag::s(3, "RustDraft Stroke"),
            Tag::s(1, "RustDraft notes"),
        ];
        upgrade(&mut tags);
        let strs: Vec<String> = tags.iter().map(Tag::str).collect();
        assert_eq!(strs, ["$DRAFTCRAFT_LAYERP", "{}", "DRAFTCRAFT_CONSTRAINTS", "DRAFTCRAFT", "Standard", "DraftCraft Stroke", "RustDraft notes"]);
    }
}
