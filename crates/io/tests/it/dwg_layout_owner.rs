//! #1057: in a DWG, entities of a layout other than the first (`*Paper_Space`) are written owned
//! by their own block (entity mode 0 and its handle). Written as "paper space" (mode 1), AutoCAD
//! put them on the first layout and left theirs empty.

use draftcraft_doc::{Circle, Common, Drawing, EntityKind, Layout, Line, Space};
use draftcraft_geom::Vec3;
use draftcraft_io::{SaveOptions, Version, read, write_with};

#[test]
fn entities_of_a_second_layout_are_owned_by_its_block() {
    let mut d = Drawing::new_metric();
    let order = d.layouts.iter().map(|l| l.tab_order).max().unwrap_or(0) + 1;
    d.layouts.push(Layout::new("SHEET_A", order));
    let line = |a: f64| EntityKind::Line(Line { a: Vec3::new(a, 0.0, 0.0), b: Vec3::new(a + 10.0, 10.0, 0.0) });
    d.add(&Space::Model, Common::default(), EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 5.0 })).unwrap();
    let first = d.layouts.iter().min_by_key(|l| l.tab_order).unwrap().name.clone();
    d.add(&Space::Paper(first.clone()), Common::default(), line(0.0)).unwrap();
    d.add(&Space::Paper("SHEET_A".into()), Common::default(), line(20.0)).unwrap();
    d.add(&Space::Paper("SHEET_A".into()), Common::default(), line(40.0)).unwrap();
    let dwg = write_with(&d, "case.dwg", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();

    // As written: entity mode and owner of each entity, as the object records give them.
    use draftcraft_dwg::records::model::{NonGraphic, ObjectKind, Space as Mode};
    let file = draftcraft_dwg::read_bytes(&dwg).unwrap();
    let sheet_a = file
        .objects
        .values()
        .find_map(|o| match &o.kind {
            ObjectKind::NonGraphic(NonGraphic::Layout(l)) if l.name == "SHEET_A" => Some(l.block),
            _ => None,
        })
        .unwrap();
    let mut on_sheet_a = 0;
    let mut on_first = 0;
    // The lines only: each layout's viewport is owned the same way.
    for (o, c) in file.objects.values().filter(|o| o.type_name == "LINE").filter_map(|o| o.entity.as_ref().map(|c| (o, c))) {
        if c.space == Mode::Owned && o.owner == sheet_a {
            on_sheet_a += 1;
        } else if c.space == Mode::Paper {
            on_first += 1;
        }
    }
    assert_eq!(on_sheet_a, 2, "the entities of SHEET_A are owned by its block");
    assert_eq!(on_first, 1, "the first layout's entities stay paper space");

    // Read back, each layout has its own.
    let back = read(&dwg, "case.dwg").unwrap();
    let count = |n: &str| back.layouts.iter().find(|l| l.name == n).map_or(0, |l| l.entities.len());
    assert_eq!((count(&first), count("SHEET_A"), back.model.len()), (1, 2, 1));
}
