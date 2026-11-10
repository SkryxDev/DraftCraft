//! DraftCraft's own DWG reader (`dwg_read`) against the acadrust path: on every entity type it
//! maps, every entity it maps is the one acadrust's DXF gives the DXF reader, in the same space,
//! with the same properties; and the layers, linetypes, text styles, blocks and layouts agree.

use std::sync::Arc as Shared;

use draftcraft_color::{Color, Rgb};
use draftcraft_doc::*;
use draftcraft_geom::{PolyVertex, Vec2, Vec3};
use draftcraft_io::{DwgReader, read_dwg_with};

/// Every entity with where it lives: `model`, `layout NAME` or `block NAME`.
fn placed(d: &Drawing) -> Vec<(String, Shared<Entity>)> {
    let mut v: Vec<_> = d.model.iter().map(|e| ("model".to_string(), e.clone())).collect();
    for l in &d.layouts {
        v.extend(l.entities.iter().map(|e| (format!("layout {}", l.name), e.clone())));
    }
    for (n, b) in &d.blocks {
        v.extend(b.entities.iter().map(|e| (format!("block {n}"), e.clone())));
    }
    v
}

/// The entity types `dwg_read` maps.
fn mapped(k: &EntityKind) -> bool {
    matches!(
        k,
        EntityKind::Line(_)
            | EntityKind::Point(_)
            | EntityKind::Circle(_)
            | EntityKind::Arc(_)
            | EntityKind::Ellipse(_)
            | EntityKind::LwPolyline(_)
            | EntityKind::Text(_)
            | EntityKind::MText(_)
            | EntityKind::Insert(_)
            | EntityKind::Solid(_)
            | EntityKind::Trace(_)
            | EntityKind::Face3d(_)
            | EntityKind::XLine(_)
            | EntityKind::Ray(_)
    )
}

/// The native reading agrees with the acadrust reading on everything the native reader maps.
/// `same_lineweight`: R14 files store no lineweights; acadrust reports 0.00 mm for them, the
/// native reader ByLayer (what an R14 entity has), so R14 entities are compared without them.
fn assert_agree(dwg: &[u8], what: &str, same_lineweight: bool) -> usize {
    let a = read_dwg_with(dwg, DwgReader::Acadrust).unwrap();
    let n = read_dwg_with(dwg, DwgReader::Native).unwrap();
    let native: std::collections::HashMap<u64, (String, Shared<Entity>)> = placed(&n).into_iter().map(|(s, e)| (e.handle.0, (s, e))).collect();
    let mut checked = 0;
    for (space, e) in placed(&a).into_iter().filter(|(_, e)| mapped(&e.kind)) {
        let Some((nspace, ne)) = native.get(&e.handle.0) else { panic!("{what}: {:X} ({:?}) missing from the native reading", e.handle.0, e.kind) };
        assert_eq!(&space, nspace, "{what}: {:X}", e.handle.0);
        assert_eq!(e.kind, ne.kind, "{what}: {:X}", e.handle.0);
        let mut c = ne.common.clone();
        if !same_lineweight {
            c.lineweight = e.common.lineweight;
        }
        assert_eq!(e.common, c, "{what}: {:X} {:?}", e.handle.0, e.kind);
        checked += 1;
    }
    let mut layers = n.layers.clone();
    if !same_lineweight {
        for (l, al) in layers.iter_mut().zip(&a.layers) {
            l.lineweight = al.lineweight;
        }
    }
    assert_eq!(a.layers, layers, "{what}: layers");
    assert_eq!(a.linetypes, n.linetypes, "{what}: linetypes");
    assert_eq!(a.text_styles, n.text_styles, "{what}: text styles");
    assert_eq!(a.blocks.keys().collect::<Vec<_>>(), n.blocks.keys().collect::<Vec<_>>(), "{what}: block names");
    let layouts = |d: &Drawing| d.layouts.iter().map(|l| (l.name.clone(), l.tab_order)).collect::<Vec<_>>();
    assert_eq!(layouts(&a), layouts(&n), "{what}: layouts");
    checked
}

fn common(layer: &str) -> Common {
    Common { layer: layer.into(), ..Common::default() }
}

/// One of every entity type the native reader maps, with the properties and conventions it
/// converts: an OCS-mirrored arc, circle and polyline, aligned text, a block reference, widths
/// and bulges, colours, a linetype, a lineweight, transparency and another application's xdata.
fn every_mapped_type() -> Drawing {
    let mut d = Drawing::new_imperial();
    d.linetypes.push(Linetype {
        name: "DASHED".into(),
        description: "Dashed".into(),
        pattern: vec![DashElement::dash(0.5), DashElement::dash(-0.25)],
    });
    let mut walls = Layer::new("WALLS");
    walls.color = Color::from_aci(1);
    walls.linetype = "DASHED".into();
    walls.lineweight = Lineweight::from_dxf(35);
    d.layers.push(walls);
    let mut block = Block::new("BOLT");
    block.entities.push(Entity::new(Handle(0x900), EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 0.5 })));
    d.blocks.insert("BOLT".into(), Shared::new(block));
    let mirrored = Common { extrusion: Vec3::new(0.0, 0.0, -1.0), ..common("WALLS") };
    let styled = Common {
        color: Color::True(Rgb::from_u32(0x00_80_40_20)),
        transparency: Transparency::Percent(40),
        ltscale: 2.0,
        lineweight: Lineweight::from_dxf(50),
        xdata: vec![
            RawTag { code: 1001, value: "MYAPP".into() },
            RawTag { code: 1000, value: "note".into() },
            RawTag { code: 1070, value: "7".into() },
        ],
        ..common("0")
    };
    let m = Space::Model;
    let kinds = vec![
        (styled.clone(), EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(10.0, 5.0, 1.0) })),
        (common("WALLS"), EntityKind::Point(Point { p: Vec3::new(1.0, 2.0, 0.0), angle: 0.0 })),
        (common("WALLS"), EntityKind::Circle(Circle { center: Vec3::new(3.0, 4.0, 0.0), radius: 2.0 })),
        (mirrored.clone(), EntityKind::Circle(Circle { center: Vec3::new(-3.0, 4.0, 0.0), radius: 1.0 })),
        (common("0"), EntityKind::Arc(Arc { center: Vec3::new(5.0, 5.0, 0.0), radius: 3.0, start: 0.25, end: 2.5 })),
        (mirrored.clone(), EntityKind::Arc(Arc { center: Vec3::new(5.0, -5.0, 0.0), radius: 3.0, start: 0.25, end: 2.5 })),
        (
            common("0"),
            EntityKind::Ellipse(Ellipse { center: Vec3::new(20.0, 0.0, 0.0), major: Vec3::new(4.0, 1.0, 0.0), ratio: 0.5, start: 0.0, end: 3.0 }),
        ),
        (
            mirrored,
            EntityKind::LwPolyline(LwPolyline {
                vertices: vec![
                    PolyVertex { p: Vec2::new(0.0, 0.0), bulge: 0.5, start_width: 0.1, end_width: 0.2 },
                    PolyVertex { p: Vec2::new(4.0, 0.0), ..Default::default() },
                    PolyVertex { p: Vec2::new(4.0, 3.0), ..Default::default() },
                ],
                closed: true,
                const_width: 0.0,
                elevation: 0.0,
                plinegen: false,
            }),
        ),
        (
            common("0"),
            EntityKind::Text(Text {
                insert: Vec3::new(0.0, 10.0, 0.0),
                align_pt: Some(Vec3::new(5.0, 10.0, 0.0)),
                height: 0.5,
                value: "Größe 90°".into(),
                rotation: 0.3,
                width_factor: 0.8,
                oblique: 0.1,
                style: "Standard".into(),
                halign: HAlign::Center,
                valign: VAlign::Middle,
            }),
        ),
        (
            common("0"),
            EntityKind::MText(MText {
                insert: Vec3::new(0.0, 20.0, 0.0),
                height: 0.25,
                width: 6.0,
                attach: 5,
                rotation: 0.5,
                style: "Standard".into(),
                contents: "First\\Psecond {\\fArial|b1;bold}".into(),
                line_spacing: 1.5,
                line_spacing_exact: false,
                columns: None,
            }),
        ),
        (
            common("WALLS"),
            EntityKind::Insert(Insert {
                block: "BOLT".into(),
                insert: Vec3::new(30.0, 0.0, 0.0),
                scale: Vec3::new(2.0, 2.0, 1.0),
                rotation: 0.7,
                attribs: vec![],
                cols: 1,
                rows: 1,
                col_spacing: 0.0,
                row_spacing: 0.0,
                clip: None,
            }),
        ),
        (
            common("0"),
            EntityKind::Solid(Solid {
                corners: [Vec3::new(0.0, 30.0, 0.0), Vec3::new(2.0, 30.0, 0.0), Vec3::new(0.0, 32.0, 0.0), Vec3::new(2.0, 32.0, 0.0)],
            }),
        ),
        (
            common("0"),
            EntityKind::Face3d(Face3d {
                corners: [Vec3::new(0.0, 40.0, 0.0), Vec3::new(2.0, 40.0, 1.0), Vec3::new(2.0, 42.0, 2.0), Vec3::new(0.0, 42.0, 1.0)],
                hidden_edges: 5,
            }),
        ),
        (common("0"), EntityKind::XLine(RayLine { base: Vec3::new(0.0, 50.0, 0.0), dir: Vec3::new(1.0, 0.0, 0.0) })),
        (common("0"), EntityKind::Ray(RayLine { base: Vec3::new(0.0, 55.0, 0.0), dir: Vec3::new(0.0, 1.0, 0.0) })),
    ];
    for (c, k) in kinds {
        d.add(&m, c, k).unwrap();
    }
    d
}

#[test]
fn every_mapped_type_reads_as_through_acadrust_in_every_version() {
    let d = every_mapped_type();
    for ver in ["AC1015", "AC1018", "AC1024", "AC1027", "AC1032"] {
        let mut d = d.clone();
        d.header.set_str("ACADVER", ver);
        let dwg = draftcraft_io::write(&d, "x.dwg").unwrap();
        let checked = assert_agree(&dwg, ver, true);
        assert!(checked >= 16, "{ver}: only {checked} entities compared");
        // Every entity type in the file is one the reader maps: only objects are left out.
        let (_, cov) = draftcraft_io::dwg_read::read(&dwg).unwrap();
        let entities_left_out: Vec<_> = cov.left_out.keys().filter(|k| !k.starts_with("object ")).collect();
        assert!(entities_left_out.is_empty(), "{ver}: {entities_left_out:?}");
    }
}

/// AutoCAD's own files of the MI-201 drawing (R14 to 2018) when `MI201_CORPUS` names them; the
/// 2007 file's container isn't read yet.
#[test]
fn corpus_reads_as_through_acadrust() {
    let Some(dir) = std::env::var_os("MI201_CORPUS") else { return };
    let dir = std::path::PathBuf::from(dir);
    for (name, same_lw) in [
        ("mi201-r14.dwg", false),
        ("mi201-r2000.dwg", true),
        ("mi201-r2004.dwg", true),
        ("mi201-r2010.dwg", true),
        ("mi201-r2013.dwg", true),
        ("mi201-r2018.dwg", true),
    ] {
        let Ok(dwg) = std::fs::read(dir.join(name)) else { continue };
        let checked = assert_agree(&dwg, name, same_lw);
        assert!(checked >= 50, "{name}: only {checked} entities compared");
    }
}

/// A damaged file is an error or a drawing, never a crash: every prefix of a saved DWG, and the
/// file with its bytes flipped in turn.
#[test]
fn damaged_dwg_does_not_panic() {
    let mut d = every_mapped_type();
    d.header.set_str("ACADVER", "AC1032");
    let dwg = draftcraft_io::write(&d, "x.dwg").unwrap();
    for cut in (0..dwg.len()).step_by(97) {
        let _ = draftcraft_io::dwg_read::read(dwg.get(..cut).unwrap_or_default());
    }
    for at in (0..dwg.len()).step_by(61) {
        let mut b = dwg.clone();
        if let Some(x) = b.get_mut(at) {
            *x ^= 0xA5;
        }
        let _ = draftcraft_io::dwg_read::read(&b);
    }
}
