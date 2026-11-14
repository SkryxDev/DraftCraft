//! TABLE heights inherit from the table style unless the file records a cell override.
#![cfg(not(target_arch = "wasm32"))]

use draftcraft_doc::{Common, Drawing, EntityKind, Space, Table, TableCell, TableStyle};

fn drawing() -> Drawing {
    let mut d = Drawing::new_imperial();
    d.table_styles.push(TableStyle { name: "Small".into(), text_height: 0.0625, title: false, header: false, ..TableStyle::default() });
    let table = Table {
        style: "Small".into(),
        text_height: 0.0625,
        title: false,
        header: false,
        row_heights: vec![0.2],
        col_widths: vec![2.0; 3],
        cells: vec![vec![
            TableCell { text: "plain".into(), ..TableCell::default() },
            TableCell { text: "{\\C256;formatted}".into(), ..TableCell::default() },
            TableCell { text: "explicit".into(), text_height: Some(0.125), ..TableCell::default() },
        ]],
        ..Table::default()
    };
    d.add(&Space::Model, Common::default(), EntityKind::Table(table)).unwrap();
    d
}

fn check(d: &Drawing) {
    let t = d
        .model
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Table(t) => Some(t),
            _ => None,
        })
        .unwrap();
    assert_eq!(t.style, "Small");
    assert_eq!(t.text_height, 0.0625);
    for cell in &t.cells[0][..2] {
        assert!(cell.text_height.is_none(), "unexpected override: {:?}", cell.text_height);
    }
    assert_eq!(t.cells[0][2].text_height, Some(0.125));
}

/// Through DWG, in the versions of both table layouts (the cells' own fields up to 2007, the
/// table content from 2010): a cell's own height stays its own, the others inherit.
#[test]
fn table_cell_heights_keep_inheritance_through_dwg() {
    use draftcraft_io::{SaveOptions, Version};
    for version in [Version::R2000, Version::R2007, Version::R2010, Version::R2018] {
        let opts = SaveOptions { version: Some(version), binary: false };
        let bytes = draftcraft_io::write_with(&drawing(), "height.dwg", &opts).unwrap();
        check(&draftcraft_io::read(&bytes, "height.dwg").unwrap());
    }
}

#[test]
fn table_cell_heights_keep_inheritance_through_dxf() {
    let bytes = draftcraft_io::write(&drawing(), "height.dxf").unwrap();
    check(&draftcraft_io::read(&bytes, "height.dxf").unwrap());
}
