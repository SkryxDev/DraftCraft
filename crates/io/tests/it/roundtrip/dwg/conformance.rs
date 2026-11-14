//! The writer's objects as the file holds them: checked on the object records the DWG reader
//! decodes, without the drawing DraftCraft builds from them. All fixtures are synthetic.

use super::super::cases;
use draftcraft_doc::Common;
use draftcraft_dwg::records::geom::Vec3;
use draftcraft_dwg::records::model::ObjectKind;
use draftcraft_io::{SaveOptions, Version, write_with};

#[test]
fn writer_populates_native_geometry_without_our_reader() {
    let source = cases::common_case("native-writer", Common::default());
    for version in Version::ALL {
        let bytes = write_with(&source.drawing, "memory.dwg", &SaveOptions { version: Some(version), binary: false }).unwrap();
        let native = draftcraft_dwg::read_bytes(&bytes).unwrap();
        let entities: Vec<&ObjectKind> = native.model_space_entities().into_iter().map(|o| &o.kind).collect();
        assert_eq!(entities.len(), 3, "{version}");
        let ObjectKind::Line(line) = entities[0] else { panic!("expected native LINE") };
        assert_eq!(line.start, Vec3::new(-1.25, 2.5, 3.75), "{version}");
        assert_eq!(line.end, Vec3::new(8.125, -13.5, 21.25), "{version}");
        let ObjectKind::Circle(circle) = entities[1] else { panic!("expected native CIRCLE") };
        assert_eq!(circle.center, Vec3::new(-7.25, 11.5, 4.75), "{version}");
        assert_eq!(circle.radius, 3.125, "{version}");
        let ObjectKind::Text(text) = entities[2] else { panic!("expected native TEXT") };
        assert_eq!(text.value, "CAD Ø", "{version}");
        assert_eq!((text.insertion.x, text.insertion.y, text.elevation), (2.5, -3.75, 4.125), "{version}");
        assert_eq!(text.height, 2.25, "{version}");
        assert_eq!(text.width_factor, 1.0, "{version}");
        assert_eq!(text.rotation, 0.0, "{version}");
    }
}

/// A linetype element's rotation reaches the DWG as it is: DraftCraft's DXF writes group 50 in
/// degrees, as AutoCAD does (AutoCAD's DXF of ZIGZAG and BATTING has 180 where its DWG has pi).
#[test]
fn complex_linetype_rotation_is_correct_in_native_objects() {
    // Both document APIs specify radians. Do not infer correctness from a DXF round trip:
    // reader and writer can share a unit error. Public DXF references disagree about LTYPE
    // group 50, so this asserts the end-to-end native angle, not a guessed DXF encoding.
    for shape in [false, true] {
        for case in cases::properties::complex_linetypes(shape) {
            for version in Version::ALL {
                let bytes = write_with(&case.drawing, "memory.dwg", &SaveOptions { version: Some(version), binary: false }).unwrap();
                let native = draftcraft_dwg::read_bytes(&bytes).unwrap();
                let line = native
                    .objects
                    .values()
                    .find_map(|o| match &o.kind {
                        ObjectKind::Linetype(l) if l.name == "Dash_test" => Some(l),
                        _ => None,
                    })
                    .unwrap();
                let elements: Vec<_> = line.elements.iter().filter(|e| e.complex != 0).collect();
                assert_eq!(elements.len(), 1, "{} {version}", case.name);
                let rotation = elements[0].rotation;
                assert!(
                    (rotation - std::f64::consts::FRAC_PI_2).abs() <= 8.0 * f64::EPSILON,
                    "{} {version}: native rotation {rotation} radians, expected pi/2",
                    case.name
                );
            }
        }
    }
}
