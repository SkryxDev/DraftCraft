//! PDF underlays and PDFIMPORT: attaching a page (from a file and from handed-over bytes), the
//! DXF it saves as (PDFUNDERLAY, PDFDEFINITION, ACAD_PDFDEFINITIONS), the page found and drawn
//! again when the drawing is reopened, and a page imported as objects.

use std::path::{Path, PathBuf};

use draftcraft_engine::Session;
use draftcraft_engine::cmd::file::{IoHooks, PdfHooks, base64_encode, set_io, set_pdf_hooks};
use draftcraft_engine::doc::{EntityKind, Image};
use serde_json::json;

fn hooks() {
    set_io(IoHooks {
        read: |b, name| draftcraft_io::read(b, name).map_err(|e| e.to_string()),
        write: |d, name| draftcraft_io::write(d, name).map_err(|e| e.to_string()),
        plot: None,
    });
    set_pdf_hooks(PdfHooks {
        pages: |b| draftcraft_io::pdf_read::pages(b).map_err(|e| e.to_string()),
        rasterize: |b, page, dpi| draftcraft_io::pdf_read::rasterize(b, page, dpi).map_err(|e| e.to_string()),
        import: draftcraft_io::pdf_read::import_flat,
    });
}

fn folder(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("draftcraft-pdf-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A two-page PDF (US Letter, then A4 landscape) with a blue line, a red square and a text.
fn pdf() -> Vec<u8> {
    let content = b"q 2 w 0 0 1 RG 72 72 m 144 72 l S 1 0 0 rg 200 200 50 50 re f Q BT /F1 12 Tf 72 300 Td (Plan) Tj ET";
    let mut objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec(),
    ];
    let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    stream.extend_from_slice(content);
    stream.extend_from_slice(b"\nendstream");
    objs.push(stream);
    objs.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());
    objs.push(b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 842 595] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec());
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn underlays(s: &Session) -> Vec<Image> {
    s.doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|e| if let EntityKind::Image(i) = &e.kind { i.underlay.as_ref().map(|_| i.clone()) } else { None })
        .collect()
}

fn open(s: &mut Session, path: &Path) {
    s.execute("open", &json!({ "path": path.to_string_lossy() })).unwrap();
}

#[test]
fn attach_places_the_page_at_its_true_size() {
    hooks();
    let mut s = Session::new();
    // A new session's drawing is in inches: a US Letter page is 8.5 × 11.
    let r = s.execute("pdfattach", &json!({ "path": "plans/site.pdf", "data": base64_encode(&pdf()), "at": [10, 20] })).unwrap();
    assert_eq!((r["name"].as_str(), r["page"].as_u64(), r["pages"].as_u64()), (Some("site"), Some(1), Some(2)));
    let size: Vec<f64> = r["size"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert!((size[0] - 8.5).abs() < 1e-9 && (size[1] - 11.0).abs() < 1e-9, "{size:?}");
    let u = underlays(&s).pop().unwrap();
    assert_eq!((u.insert.x, u.insert.y, u.size.x, u.size.y), (10.0, 20.0, 612.0, 792.0));
    assert_eq!(u.underlay.as_ref().unwrap().page, "1");
    assert!(s.doc().unwrap().rasters.get(&u.raster_key()).is_some());
    assert_eq!(s.doc().unwrap().model.iter().next().unwrap().kind.type_name(), "PDF Underlay");

    // The second page, by ATTACH, at an explicit scale and rotation.
    s.execute("attach", &json!({ "path": "plans/site.pdf", "data": base64_encode(&pdf()), "page": 2, "scale": 1, "rotation": 90 })).unwrap();
    let u = underlays(&s).pop().unwrap();
    assert_eq!((u.size.x, u.size.y), (842.0, 595.0));
    assert!(u.u.x.abs() < 1e-12 && (u.u.y - 1.0 / 72.0).abs() < 1e-12, "{u:?}");
    // It draws with its pixels.
    let list =
        draftcraft_engine::render::build(s.doc().unwrap(), &draftcraft_engine::doc::Space::Model, &draftcraft_engine::render::Options::default());
    assert_eq!(list.images.len(), 2);

    for bad in [json!({ "path": "x.pdf", "data": base64_encode(b"not a pdf") }), json!({ "path": "x.pdf", "data": base64_encode(&pdf()), "page": 3 })]
    {
        assert!(s.execute("pdfattach", &bad).is_err(), "{bad}");
    }
    assert!(s.execute("pdfattach", &json!({ "path": "x.pdf", "data": base64_encode(&pdf()), "page": 0 })).is_err());
}

#[test]
fn an_underlay_survives_save_and_reopen_and_finds_its_page() {
    hooks();
    let dir = folder("reopen");
    std::fs::write(dir.join("site.pdf"), pdf()).unwrap();
    let dwg = dir.join("plan.dxf");
    let mut s = Session::new();
    s.execute("saveas", &json!({ "path": dwg.to_string_lossy() })).unwrap();
    s.execute("pdfattach", &json!({ "path": "site.pdf", "page": 2, "at": [5, 6], "scale": 2, "rotation": 30 })).unwrap();
    s.execute("qsave", &json!({})).unwrap();
    let text = std::fs::read_to_string(&dwg).unwrap();
    for needed in ["PDFUNDERLAY", "PDFDEFINITION", "ACAD_PDFDEFINITIONS", "AcDbUnderlayReference", "AcDbUnderlayDefinition", "AcDbPdfReference"] {
        assert!(text.contains(needed), "{needed} missing");
    }
    assert!(!text.contains("\nIMAGEDEF\r\n") && !text.contains("\nIMAGEDEF\n"), "an underlay is not an image");

    let mut s = Session::new();
    open(&mut s, &dwg);
    let u = underlays(&s).pop().unwrap();
    assert_eq!((u.path.as_str(), u.underlay.as_ref().unwrap().page.as_str()), ("site.pdf", "2"));
    assert_eq!((u.size.x, u.size.y), (842.0, 595.0), "the page size comes from the PDF");
    assert!((u.insert.x - 5.0).abs() < 1e-9 && (u.insert.y - 6.0).abs() < 1e-9);
    let a = 30f64.to_radians();
    assert!((u.u.x - a.cos() * 2.0 / 72.0).abs() < 1e-9 && (u.u.y - a.sin() * 2.0 / 72.0).abs() < 1e-9, "{u:?}");
    assert!(s.doc().unwrap().rasters.get(&u.raster_key()).is_some(), "the page is drawn again");
    assert!(!s.state().unwrap().is_dirty(), "loading the page is not a change");
    let listed = s.execute("ulayers", &json!({})).unwrap();
    assert_eq!(listed["underlays"][0]["status"], "Loaded");

    // Without its PDF, the underlay stays in the drawing (and in the file when saved again).
    std::fs::remove_file(dir.join("site.pdf")).unwrap();
    let mut s = Session::new();
    open(&mut s, &dwg);
    assert_eq!(underlays(&s).len(), 1);
    assert_eq!(s.execute("ulayers", &json!({})).unwrap()["underlays"][0]["status"], "Not Found");
    let again = dir.join("again.dxf");
    s.execute("saveas", &json!({ "path": again.to_string_lossy() })).unwrap();
    assert!(std::fs::read_to_string(&again).unwrap().contains("PDFUNDERLAY"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pdfimport_turns_a_page_into_objects() {
    hooks();
    let mut s = Session::new();
    s.execute("pdfattach", &json!({ "path": "site.pdf", "data": base64_encode(&pdf()), "at": [100, 0], "scale": 72 })).unwrap();
    // The underlay's page from its file: the test hands the bytes over instead.
    let r = s.execute("pdfimport", &json!({ "path": "site.pdf", "data": base64_encode(&pdf()), "at": [100, 0], "scale": 72 })).unwrap();
    assert_eq!(r["counts"], json!({ "vectors": 1, "fills": 1, "text": 1 }));
    let d = s.doc().unwrap();
    let mut layers: Vec<String> = d.model.iter().filter(|e| !matches!(e.kind, EntityKind::Image(_))).map(|e| e.common.layer.clone()).collect();
    layers.sort();
    assert_eq!(layers, ["PDF_Geometry", "PDF_Solid Fills", "PDF_Text"]);
    // At 72 units per inch one point is one unit: the line runs from (172, 72) to (244, 72).
    let line = d.model.iter().find_map(|e| if let EntityKind::Line(l) = &e.kind { Some(l.clone()) } else { None }).unwrap();
    assert_eq!([(line.a.x, line.a.y), (line.b.x, line.b.y)], [(172.0, 72.0), (244.0, 72.0)]);
    let text = d.model.iter().find_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t.clone()) } else { None }).unwrap();
    assert_eq!(text.value, "Plan");
    assert!((text.insert.x - 172.0).abs() < 1e-6 && (text.insert.y - 300.0).abs() < 1e-6);
    // Undo takes the whole import back in one step.
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 1);
    assert!(s.execute("pdfimport", &json!({ "handle": "FFFFF" })).is_err());
}

#[test]
fn an_underlay_survives_a_dwg_round_trip() {
    hooks();
    let dir = folder("dwg");
    std::fs::write(dir.join("site.pdf"), pdf()).unwrap();
    let dwg = dir.join("plan.dwg");
    let mut s = Session::new();
    s.execute("saveas", &json!({ "path": dwg.to_string_lossy() })).unwrap();
    s.execute("pdfattach", &json!({ "path": "site.pdf", "page": 2, "at": [5, 6], "scale": 2 })).unwrap();
    s.execute("qsave", &json!({})).unwrap();
    let mut s = Session::new();
    open(&mut s, &dwg);
    let u = underlays(&s).pop().expect("the underlay is in the DWG");
    assert_eq!((u.path.as_str(), u.underlay.as_ref().unwrap().page.as_str()), ("site.pdf", "2"));
    assert_eq!((u.size.x, u.size.y), (842.0, 595.0));
    assert!((u.insert.x - 5.0).abs() < 1e-9 && (u.u.x - 2.0 / 72.0).abs() < 1e-9, "{u:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
