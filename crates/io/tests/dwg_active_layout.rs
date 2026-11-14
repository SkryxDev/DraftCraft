//! A drawing whose active layout is not the first tab keeps it active when saved: its block is
//! the one written as `*Paper_Space` (the DWG header's paper space block), the others
//! `*Paper_SpaceN`. Saving used to make the first tab the active one again.

use draftcraft_doc::{Common, Drawing, EntityKind, Layout, Line, Space};
use draftcraft_geom::Vec3;
use draftcraft_io::{SaveOptions, Version, read, write_with};

fn three_sheets() -> Drawing {
    let mut d = Drawing::new_metric();
    d.layouts = vec![Layout::new("S1", 1), Layout::new("S2", 2), Layout::new("S3", 3)];
    let line = |a: f64| EntityKind::Line(Line { a: Vec3::new(a, 0.0, 0.0), b: Vec3::new(a + 10.0, 10.0, 0.0) });
    d.add(&Space::Paper("S1".into()), Common::default(), line(0.0)).unwrap();
    d.add(&Space::Paper("S2".into()), Common::default(), line(20.0)).unwrap();
    d.add(&Space::Paper("S2".into()), Common::default(), line(40.0)).unwrap();
    d
}

/// The layout the DWG marks active: the one whose block the header names as paper space, as
/// the object records of the file give them.
fn dwg_active_layout(dwg: &[u8]) -> String {
    use draftcraft_dwg::records::model::{NonGraphic, ObjectKind};
    let file = draftcraft_dwg::read_bytes(dwg).unwrap();
    let ps = file.header.paper_space;
    file.objects
        .values()
        .find_map(|o| match &o.kind {
            ObjectKind::NonGraphic(NonGraphic::Layout(l)) if l.block == ps => Some(l.name.clone()),
            _ => None,
        })
        .unwrap()
}

fn dwg(d: &Drawing) -> Vec<u8> {
    write_with(d, "case.dwg", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap()
}

#[test]
fn the_active_layout_stays_active_through_dwg_saves() {
    let mut d = three_sheets();
    d.active_layout = Some("S2".into());
    let first = dwg(&d);
    assert_eq!(dwg_active_layout(&first), "S2");

    // Opened again and saved again, as a drawing from another program would be.
    let back = read(&first, "case.dwg").unwrap();
    assert_eq!(back.active_layout.as_deref(), Some("S2"));
    let count = |d: &Drawing, n: &str| d.layouts.iter().find(|l| l.name == n).map_or(0, |l| l.entities.len());
    assert_eq!((count(&back, "S1"), count(&back, "S2"), count(&back, "S3")), (1, 2, 0));
    let mut tabs: Vec<(u32, &str)> = back.layouts.iter().map(|l| (l.tab_order, l.name.as_str())).collect();
    tabs.sort();
    assert_eq!(tabs.iter().map(|t| t.1).collect::<Vec<_>>(), ["S1", "S2", "S3"], "the tab order is kept");

    let second = dwg(&back);
    assert_eq!(dwg_active_layout(&second), "S2");
    let again = read(&second, "case.dwg").unwrap();
    assert_eq!(again.active_layout.as_deref(), Some("S2"));
    assert_eq!((count(&again, "S1"), count(&again, "S2"), count(&again, "S3")), (1, 2, 0));
}

#[test]
fn the_active_layout_survives_dxf_too() {
    let mut d = three_sheets();
    d.active_layout = Some("S3".into());
    let dxf = write_with(&d, "case.dxf", &SaveOptions::default()).unwrap();
    let back = read(&dxf, "case.dxf").unwrap();
    assert_eq!(back.active_layout.as_deref(), Some("S3"));
}

#[test]
fn without_an_active_layout_the_first_tab_is_active() {
    let mut d = three_sheets();
    d.active_layout = Some("gone".into());
    assert_eq!(dwg_active_layout(&dwg(&d)), "S1");
}

#[test]
fn the_active_layout_name_is_matched_ignoring_case() {
    let mut d = three_sheets();
    d.active_layout = Some("s2".into());
    assert_eq!(dwg_active_layout(&dwg(&d)), "S2");
}
