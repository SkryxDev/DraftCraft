//! A polyline's vertices come back in the order its own vertex list stores them, not sorted by
//! handle (#1856). A vertex inserted into an existing polyline gets a newer, higher handle, so a
//! handle-ordered read silently reorders the outline (wrong shape, longer length).
//!
//! The DWG is written here from records whose second 3D-polyline vertex has the highest handle:
//! exactly what an editing program produces when it inserts a vertex.
#![cfg(not(target_arch = "wasm32"))]

use draftcraft_doc::{Common, Drawing, EntityKind, Polyline3d, Space};
use draftcraft_dxf::{Tag, Version};
use draftcraft_geom::Vec3;
use draftcraft_io::dwg::write::{FileInfo, version_of, write};

const POINTS: [(f64, f64, f64); 6] = [(0.0, 0.0, 0.0), (10.0, 0.0, 1.0), (10.0, 10.0, 2.0), (0.0, 10.0, 3.0), (0.0, 20.0, 4.0), (10.0, 20.0, 5.0)];

/// A drawing with one 6-vertex 3D polyline whose vertex handles are not ascending: the second
/// vertex is the newest (highest) handle.
fn dwg_with_out_of_order_vertex_handles(version: Version) -> Vec<u8> {
    let mut d = Drawing::new_metric();
    let points = POINTS.iter().map(|&(x, y, z)| Vec3::new(x, y, z)).collect();
    d.add(&Space::Model, Common::default(), EntityKind::Polyline3d(Polyline3d { points, closed: false })).unwrap();
    let opts = draftcraft_io::SaveOptions { version: Some(Version::R2000), binary: false };
    let dxf = draftcraft_io::write_with(&d, "x.dxf", &opts).unwrap();
    let mut tags = draftcraft_dxf::parse(&dxf).unwrap();
    // The second vertex gets a handle above every other, and the seed moves past it.
    let second = tags.iter().enumerate().filter(|(_, t)| t.code == 0 && t.str() == "VERTEX").nth(1).map(|(i, _)| i).unwrap();
    let handle = tags.iter_mut().skip(second).find(|t| t.code == 5).unwrap();
    *handle = Tag::s(5, "7FFF0");
    let seed = tags.iter().position(|t| t.code == 9 && t.str() == "$HANDSEED").unwrap();
    tags[seed + 1] = Tag::s(5, "7FFF1");
    let info = FileInfo { app_name: "test", app_version: "0", preview_png: None, preview_bmp: None };
    let dwg = write(&tags, version_of(version.code()).unwrap(), &info).unwrap().0;
    let file = draftcraft_dwg::read_bytes(&dwg).unwrap();
    let vertex = file.objects.get(&draftcraft_dwg::records::model::Handle(0x7FFF0));
    assert!(vertex.is_some_and(|o| o.type_name.contains("VERTEX")), "the second vertex has the newest handle");
    dwg
}

/// Reads `dwg` through `draftcraft_io::read` and returns the points of its only 3D polyline, in the
/// order the reader produced them; panics if the drawing cannot be read or holds no 3D polyline,
/// since either would mean the test is no longer exercising vertex order.
fn polyline3d_points(dwg: &[u8]) -> Vec<Vec3> {
    let drawing = draftcraft_io::read(dwg, "x.dwg").expect("read the DWG");
    let polyline = drawing.model.iter().find_map(|e| if let EntityKind::Polyline3d(p) = &e.kind { Some(p) } else { None });
    polyline.expect("one 3D polyline").points.clone()
}

/// Regression guard for #1856, run for two DWG versions (AC1018 and AC1032): the
/// reader must keep a polyline's vertices in the order the entity's own vertex list lists them,
/// even when a mid-outline vertex carries the highest handle. Ordering by handle would move the
/// second point to the end, changing the outline's shape and length.
#[test]
fn dwg_read_keeps_polyline_vertex_order() {
    let expected: Vec<Vec3> = POINTS.iter().map(|&(x, y, z)| Vec3::new(x, y, z)).collect();
    for version in [Version::R2004, Version::R2018] {
        let dwg = dwg_with_out_of_order_vertex_handles(version);
        assert_eq!(polyline3d_points(&dwg), expected, "{version:?}");
    }
}
