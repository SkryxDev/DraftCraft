//! A closed fit-point spline is periodic: DXF flags it closed + periodic (group 70 bits 1 and 2)
//! and it reads back as the same smooth closed curve.

use rustdraft_doc::{Common, Drawing, EntityKind, Space};
use rustdraft_geom::{Spline, Vec2};

#[test]
fn closed_fit_spline_roundtrips_as_periodic() {
    let fit = [Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 3.0), Vec2::new(0.0, 3.0)];
    let sp = Spline::from_fit_points_closed(&fit);
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Spline(sp.clone())).unwrap();
    let bytes = rustdraft_io::write(&d, "a.dxf").unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    let spline = &text[text.find("SPLINE").unwrap()..];
    assert!(spline.contains("70\r\n11\r\n"), "planar + closed + periodic");
    let back = rustdraft_io::read(&bytes, "a.dxf").unwrap();
    let Some(EntityKind::Spline(r)) = back.model.iter().map(|e| e.kind.clone()).next() else { panic!("no spline") };
    assert!(r.closed && r.is_periodic());
    assert_eq!(r.fit.len(), 4);
    for i in 0..=20 {
        let t = i as f64 / 20.0;
        assert!(r.eval(t).near(sp.eval(t), 1e-9));
    }
}

/// A CONIC is a rational quadratic spline: DXF (group 41 weights, flag 4) and DWG at every
/// version keep its weights, so it reads back as the same exact ellipse, parabola or hyperbola.
#[test]
fn conic_splines_keep_their_weights() {
    let (start, apex, end) = (Vec2::new(0.0, 0.0), Vec2::new(5.0, 5.0), Vec2::new(10.0, 0.0));
    let mut d = Drawing::new_metric();
    let rhos = [0.2, 0.5, 0.8];
    for rho in rhos {
        d.add(&Space::Model, Common::default(), EntityKind::Spline(Spline::conic(start, apex, end, rho).unwrap())).unwrap();
    }
    let check = |back: &Drawing, what: &str| {
        let got: Vec<Spline> = back.model.iter().filter_map(|e| if let EntityKind::Spline(s) = &e.kind { Some(s.clone()) } else { None }).collect();
        assert_eq!(got.len(), rhos.len(), "{what}");
        for (sp, rho) in got.iter().zip(rhos) {
            let (lo, hi) = sp.domain();
            let mid = (start + end) * 0.5;
            assert!(sp.eval((lo + hi) / 2.0).near(mid + (apex - mid) * rho, 1e-9), "{what} rho {rho}");
            assert!(sp.eval(lo).near(start, 1e-9) && sp.eval(hi).near(end, 1e-9), "{what}");
        }
    };
    check(&rustdraft_io::read(&rustdraft_io::write(&d, "c.dxf").unwrap(), "c.dxf").unwrap(), "dxf");
    #[cfg(not(target_arch = "wasm32"))]
    for v in rustdraft_io::Version::ALL {
        let opts = rustdraft_io::SaveOptions { version: Some(v), binary: false };
        let dwg = rustdraft_io::write_with(&d, "c.dwg", &opts).unwrap();
        check(&rustdraft_io::read(&dwg, "c.dwg").unwrap(), &format!("dwg {v}"));
    }
}
