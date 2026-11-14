//! Literal MTEXT carets and their adjacent whitespace survive DXF and DWG saves (#2250).

use draftcraft_doc::{Common, Drawing, EntityKind, Handle, MText, Space};
use draftcraft_geom::Vec3;
use draftcraft_io::{SaveOptions, Version, read, write_with};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn drawing() -> Result<(Drawing, Vec<(Handle, String)>), Box<dyn std::error::Error>> {
    let mut d = Drawing::new_metric();
    let mut expected = Vec::new();
    let mut cases: Vec<String> = [
        "ordinary text",
        "^ C",
        "A ^ C B",
        "^  C",
        "A ^   C  B",
        "^ ",
        "^^ C ^ ^  D^",
        "^I ^J ^ C",
        "\t^ C\nD\tE",
        "^\t ^\n C",
        r"A\P^ C\P^I ^J",
        r"\S+0.1^-0.1; ^ C",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    // Exercise multiple group-3 chunks and a caret at the output chunk boundary.
    cases.push(format!("{}^ C", "A".repeat(124)));
    cases.push(format!("{}^ C B", "A".repeat(260)));
    for contents in cases {
        let kind = EntityKind::MText(MText {
            insert: Vec3::new(1.0, 2.0, 0.0),
            height: 0.5,
            width: 12.5,
            attach: 1,
            rotation: 0.0,
            style: "Standard".into(),
            contents: contents.clone(),
            line_spacing: 1.0,
            line_spacing_exact: false,
            columns: None,
        });
        let handle = d.add(&Space::Model, Common::default(), kind)?;
        expected.push((handle, contents));
    }
    Ok((d, expected))
}

fn assert_contents(d: &Drawing, expected: &[(Handle, String)], context: &str) -> TestResult {
    assert_eq!(d.model.len(), expected.len(), "{context}: entity count");
    for (handle, contents) in expected {
        let e = d.entity(*handle).ok_or("MTEXT handle lost")?;
        let EntityKind::MText(t) = &e.kind else { return Err("MTEXT entity type changed".into()) };
        assert_eq!(&t.contents, contents, "{context}: {handle:?}");
    }
    Ok(())
}

#[test]
fn dxf_mtext_caret_spaces_round_trip() -> TestResult {
    let (original, expected) = drawing()?;
    for version in Version::ALL {
        for binary in [false, true] {
            let mut d = original.clone();
            for pass in 1..=2 {
                let bytes = write_with(&d, "carets.dxf", &SaveOptions { version: Some(version), binary })?;
                d = read(&bytes, "carets.dxf")?;
                assert_contents(&d, &expected, &format!("{version:?}, binary={binary}, pass={pass}"))?;
            }
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn dwg_mtext_caret_spaces_round_trip() -> TestResult {
    let (original, expected) = drawing()?;
    for version in Version::ALL {
        let mut d = original.clone();
        for pass in 1..=2 {
            let bytes = write_with(&d, "carets.dwg", &SaveOptions { version: Some(version), binary: false })?;
            assert_eq!(draftcraft_dwg::version(&bytes).as_deref(), Some(version.code()));
            // Inspect the stored DWG text as its object records hold it, before the drawing is
            // built from them: a compensating reader must not hide corruption on save.
            let stored = stored_contents(&bytes)?;
            for (handle, contents) in &expected {
                assert_eq!(stored.get(&handle.0), Some(contents), "stored DWG {version:?}, pass={pass}, {handle:?}");
            }
            d = read(&bytes, "carets.dwg")?;
            assert_contents(&d, &expected, &format!("DWG {version:?}, pass={pass}"))?;
        }
    }
    Ok(())
}

/// Each MTEXT's contents as the DWG's object records hold them, by handle.
#[cfg(not(target_arch = "wasm32"))]
fn stored_contents(dwg: &[u8]) -> Result<std::collections::HashMap<u64, String>, Box<dyn std::error::Error>> {
    use draftcraft_dwg::records::model::ObjectKind;
    let file = draftcraft_dwg::read_bytes(dwg)?;
    Ok(file
        .objects
        .values()
        .filter_map(|o| match &o.kind {
            ObjectKind::MText(t) => Some((o.handle.0, t.contents.clone())),
            _ => None,
        })
        .collect())
}

/// A literal caret and the space after it: DXF escapes the caret's space once (two spaces on the
/// wire), the DWG holds the text as it is.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_caret_space_is_escaped_once_in_dxf_and_stored_as_is_in_dwg() -> TestResult {
    let (d, _) = drawing()?;
    let dxf = draftcraft_io::write_dxf(&d);
    assert!(dxf.contains("  1\r\n^  C\r\n"), "literal caret plus authored space needs two wire spaces");
    let dwg = write_with(&d, "carets.dwg", &SaveOptions { version: Some(Version::R2018), binary: false })?;
    assert!(stored_contents(&dwg)?.values().any(|t| t == "^ C"));
    Ok(())
}
