//! Open-speed benchmark for the two DWG readers (`docs/dwg.md`).
//!
//! Builds a drawing of lines, arcs, circles and text (200,000 entities by default), saves it as
//! a 2018 DWG, then opens it with acadrust and with DraftCraft's own reader and says how long
//! each took and whether every entity the native reader maps came out the same.
//!
//! ```text
//! cargo run --release -p draftcraft-io --example dwg_bench -- [entities] [--keep out.dwg]
//! ```

use std::collections::HashMap;
use std::time::Instant;

use draftcraft_doc::{Arc, Circle, Common, Drawing, EntityKind, HAlign, Line, Space, Text, VAlign};
use draftcraft_geom::Vec3;
use draftcraft_io::{DwgReader, read_dwg_with};

fn drawing(n: usize) -> Drawing {
    let mut d = Drawing::new_imperial();
    d.header.set_str("ACADVER", "AC1032");
    let side = ((n / 4) as f64).sqrt().ceil().max(1.0) as usize;
    for i in 0..n {
        let cell = i / 4;
        let (x, y) = ((cell % side) as f64 * 40.0, (cell / side) as f64 * 40.0);
        let kind = match i % 4 {
            0 => EntityKind::Line(Line { a: Vec3::new(x, y, 0.0), b: Vec3::new(x + 30.0, y + 10.0, 0.0) }),
            1 => EntityKind::Arc(Arc { center: Vec3::new(x + 10.0, y + 20.0, 0.0), radius: 6.0, start: 0.3, end: 2.8 }),
            2 => EntityKind::Circle(Circle { center: Vec3::new(x + 25.0, y + 25.0, 0.0), radius: 4.0 }),
            _ => EntityKind::Text(Text {
                insert: Vec3::new(x, y + 32.0, 0.0),
                align_pt: None,
                height: 2.5,
                value: format!("PART {cell}"),
                rotation: 0.0,
                width_factor: 1.0,
                oblique: 0.0,
                style: "Standard".into(),
                halign: HAlign::Left,
                valign: VAlign::Baseline,
            }),
        };
        let _ = d.add(&Space::Model, Common::default(), kind);
    }
    d
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let n = args.first().and_then(|a| a.parse().ok()).unwrap_or(200_000);
    let keep = args.iter().position(|a| a == "--keep").and_then(|i| args.get(i + 1));

    let t = Instant::now();
    let d = drawing(n);
    eprintln!("built {} entities in {:.0} ms", d.entity_count(), ms(t));
    let t = Instant::now();
    let dwg = draftcraft_io::write(&d, "bench.dwg")?;
    eprintln!("saved a {:.1} MB DWG 2018 in {:.0} ms", dwg.len() as f64 / 1048576.0, ms(t));
    if let Some(path) = keep {
        std::fs::write(path, &dwg)?;
    }

    let t = Instant::now();
    let a = read_dwg_with(&dwg, DwgReader::Acadrust)?;
    let ta = ms(t);
    let t = Instant::now();
    let n2 = read_dwg_with(&dwg, DwgReader::Native)?;
    let tn = ms(t);

    let native: HashMap<u64, _> = n2.model.iter().map(|e| (e.handle.0, e.clone())).collect();
    let same = a.model.iter().filter(|e| native.get(&e.handle.0).is_some_and(|x| x.kind == e.kind && x.common == e.common)).count();
    println!("open, acadrust: {ta:>7.0} ms");
    println!("open, native:   {tn:>7.0} ms  ({:.1}x)", ta / tn.max(1e-9));
    println!("identical entities: {same} of {}", a.model.len());
    Ok(())
}
