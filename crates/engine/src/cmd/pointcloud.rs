//! POINTCLOUDATTACH: bring a point cloud into the drawing as points. DraftCraft reads XYZ text
//! files (`x y z [r g b]` per line, spaces, commas, semicolons or tabs between the values; also
//! .pts and .csv) and LAS files (ASPRS LAS 1.0–1.4, point formats 0–10, from the published
//! specification); AutoCAD's own .rcp/.rcs are not published and stay "not available yet".
//!
//! The points go into a block named after the file and one reference to it is inserted, so the
//! cloud moves, hides (by its layer) and erases as one object. A cloud larger than `maxPoints` is
//! thinned evenly. The file is hostile input: its size, the point count and every number are
//! checked, and points with non-finite coordinates are skipped.

use std::sync::Arc;

use draftcraft_color::{Color, Rgb};
use draftcraft_doc::{Block, Entity, EntityKind, Insert, Point};
use draftcraft_geom::{Vec2, Vec3};
use serde_json::{Value, json};

use super::file::base64_decode;
use crate::cmd::{CommandSpec, bad, point_value, str_param};
use crate::{Result, Session};

/// Largest point cloud file read.
const MAX_FILE_BYTES: u64 = 512 << 20;
/// Default and largest number of points kept.
const DEFAULT_POINTS: usize = 100_000;
const MAX_POINTS: usize = 1_000_000;
/// Most text lines looked at in an XYZ file.
const MAX_LINES: usize = 50_000_000;

const ID: &str = "pointcloudattach";

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new(ID, "Point Cloud...", run_attach).menu(&["Insert", "Point Cloud..."]).alias(&["pcattach", "pointcloud"]).params(
        "{path: .xyz/.pts/.csv/.txt or .las file, data?: base64 file contents, at?: [x,y] (default 0,0), scale?: number (default 1), \
                 maxPoints?: 1..1000000 (default 100000; larger clouds are thinned evenly), name?: block name (default the file name)} → \
                 {handle, block, points, total, colored}",
    )]
}

/// A point of the cloud and its colour, if the file has one.
type CloudPoint = (Vec3, Option<Rgb>);

/// A text file's points: up to `keep`, every `stride`-th of the valid lines. Returns the points
/// and how many valid lines there were.
fn parse_xyz(text: &str, keep: usize) -> (Vec<CloudPoint>, usize) {
    let lines = || {
        text.lines().take(MAX_LINES).filter_map(|l| {
            let nums: Vec<f64> = l
                .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';'))
                .filter(|t| !t.is_empty())
                .map_while(|t| t.parse::<f64>().ok())
                .collect();
            let (x, y, z) = (*nums.first()?, *nums.get(1)?, nums.get(2).copied().unwrap_or(0.0));
            if !(x.is_finite() && y.is_finite() && z.is_finite()) {
                return None;
            }
            // Colour: the last three numbers when there are at least six (x y z [intensity] r g b).
            let rgb = (nums.len() >= 6)
                .then(|| nums.get(nums.len() - 3..))
                .flatten()
                .filter(|c| c.iter().all(|v| (0.0..=255.0).contains(v)))
                .map(|c| Rgb(c[0] as u8, c[1] as u8, c[2] as u8));
            Some((Vec3::new(x, y, z), rgb))
        })
    };
    let total = lines().count();
    let stride = total.div_ceil(keep.max(1)).max(1);
    (lines().step_by(stride).take(keep).collect(), total)
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at.checked_add(2)?)?.try_into().ok()?))
}
fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?))
}
fn i32_at(b: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?))
}
fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at.checked_add(8)?)?.try_into().ok()?))
}
fn f64_at(b: &[u8], at: usize) -> Option<f64> {
    Some(f64::from_le_bytes(b.get(at..at.checked_add(8)?)?.try_into().ok()?))
}

/// A LAS file's points (public ASPRS LAS specification): up to `keep`, evenly thinned.
fn parse_las(b: &[u8], keep: usize) -> std::result::Result<(Vec<CloudPoint>, usize), String> {
    if b.get(0..4) != Some(b"LASF") {
        return Err("not a LAS file".into());
    }
    let bad = || "a damaged LAS header".to_string();
    let minor = *b.get(25).ok_or_else(bad)?;
    let offset = usize::try_from(u32_at(b, 96).ok_or_else(bad)?).map_err(|_| bad())?;
    // Bits 6 and 7 of the format byte mark compressed (LAZ) data, which isn't read.
    let format_byte = *b.get(104).ok_or_else(bad)?;
    if format_byte & 0xC0 != 0 {
        return Err("compressed LAS (LAZ) files are not available yet".into());
    }
    let format = format_byte & 0x3F;
    let record = usize::from(u16_at(b, 105).ok_or_else(bad)?);
    let legacy = u64::from(u32_at(b, 107).ok_or_else(bad)?);
    let count = if legacy == 0 && minor >= 4 { u64_at(b, 247).ok_or_else(bad)? } else { legacy };
    let scale = [f64_at(b, 131), f64_at(b, 139), f64_at(b, 147)];
    let origin = [f64_at(b, 155), f64_at(b, 163), f64_at(b, 171)];
    let (Some(sx), Some(sy), Some(sz)) = (scale[0], scale[1], scale[2]) else { return Err(bad()) };
    let (Some(ox), Some(oy), Some(oz)) = (origin[0], origin[1], origin[2]) else { return Err(bad()) };
    if ![sx, sy, sz, ox, oy, oz].iter().all(|v| v.is_finite()) {
        return Err(bad());
    }
    // Where each point format keeps red, green and blue.
    let rgb_at = match format {
        2 => Some(20),
        3 | 5 => Some(28),
        7 | 8 | 10 => Some(30),
        0 | 1 | 4 | 6 | 9 => None,
        _ => return Err(format!("LAS point format {format} is not available yet")),
    };
    let min_record = rgb_at.map_or(12, |at| at + 6);
    if record < min_record {
        return Err(bad());
    }
    // Only the records the file really holds.
    let fits = b.len().saturating_sub(offset) / record;
    let total = usize::try_from(count).unwrap_or(usize::MAX).min(fits);
    let stride = total.div_ceil(keep.max(1)).max(1);
    // 16-bit colours are scaled to 8 bits; files with 8-bit values in 16-bit fields are detected.
    let mut out = Vec::with_capacity(total.min(keep));
    let mut wide = false;
    for i in (0..total).step_by(stride).take(keep) {
        let Some(at) = i.checked_mul(record).and_then(|r| r.checked_add(offset)) else { break };
        let (Some(x), Some(y), Some(z)) = (i32_at(b, at), i32_at(b, at + 4), i32_at(b, at + 8)) else { break };
        let p = Vec3::new(f64::from(x) * sx + ox, f64::from(y) * sy + oy, f64::from(z) * sz + oz);
        if !(p.x.is_finite() && p.y.is_finite() && p.z.is_finite()) {
            continue;
        }
        let rgb = rgb_at.and_then(|c| Some([u16_at(b, at + c)?, u16_at(b, at + c + 2)?, u16_at(b, at + c + 4)?]));
        wide |= rgb.is_some_and(|c| c.iter().any(|v| *v > 255));
        out.push((p, rgb.map(|c| Rgb(c[0] as u8, c[1] as u8, c[2] as u8)), rgb));
    }
    let points = out
        .into_iter()
        .map(|(p, low, full)| {
            let c = if wide { full.map(|c| Rgb((c[0] >> 8) as u8, (c[1] >> 8) as u8, (c[2] >> 8) as u8)) } else { low };
            (p, c)
        })
        .collect();
    Ok((points, total))
}

fn read(path: &str, host: Option<&str>) -> std::result::Result<Vec<u8>, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let file = super::blocks::xref::find_file(path, host).ok_or("file not found")?;
        let limit = usize::try_from(MAX_FILE_BYTES).unwrap_or(usize::MAX);
        crate::capped_read::read_capped(&file, limit).map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData { format!("the file is larger than {} MB", MAX_FILE_BYTES >> 20) } else { e.to_string() }
        })
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (path, host);
        Err("files are not available on the web: pass the file as `data`".into())
    }
}

fn run_attach(s: &mut Session, p: &Value) -> Result<Value> {
    let path = str_param(p, "path").filter(|x| !x.trim().is_empty()).ok_or_else(|| bad(ID, "`path` names the point cloud file"))?.trim().to_string();
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".rcp") || lower.ends_with(".rcs") {
        return Err(bad(ID, "ReCap point clouds (.rcp, .rcs) are not available yet: export the scan as LAS or XYZ"));
    }
    let keep = match p.get("maxPoints") {
        None => DEFAULT_POINTS,
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| (1..=MAX_POINTS).contains(n))
            .ok_or_else(|| bad(ID, format!("`maxPoints` must be 1 to {MAX_POINTS}")))?,
    };
    let at = match p.get("at") {
        Some(v) => point_value(v).ok_or_else(|| bad(ID, "`at` must be [x, y]"))?,
        None => Vec2::ZERO,
    };
    let scale = match p.get("scale") {
        Some(v) => v.as_f64().filter(|x| x.is_finite() && *x > 0.0).ok_or_else(|| bad(ID, "`scale` must be greater than zero"))?,
        None => 1.0,
    };
    let bytes = match p.get("data") {
        Some(d) => {
            base64_decode(d.as_str().ok_or_else(|| bad(ID, "`data` must be base64 text"))?).ok_or_else(|| bad(ID, "`data` is not valid base64"))?
        }
        None => {
            let host = s.state()?.path.clone();
            read(&path, host.as_deref()).map_err(|e| bad(ID, format!("{path}: {e}")))?
        }
    };
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(bad(ID, format!("the file is larger than {} MB", MAX_FILE_BYTES >> 20)));
    }
    let (points, total) = if bytes.starts_with(b"LASF") || lower.ends_with(".las") {
        parse_las(&bytes, keep).map_err(|e| bad(ID, format!("{path}: {e}")))?
    } else {
        parse_xyz(&String::from_utf8_lossy(&bytes), keep)
    };
    if points.is_empty() {
        return Err(bad(ID, format!("{path}: no points found")));
    }
    let stem = path.rsplit(['/', '\\']).next().unwrap_or(&path);
    let stem = stem.rsplit_once('.').map(|(s, _)| s).filter(|s| !s.is_empty()).unwrap_or(stem);
    let wanted = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| format!("PointCloud_{stem}"));
    let wanted: String = wanted.chars().map(|c| if c.is_alphanumeric() || matches!(c, '_' | '-' | ' ' | '$') { c } else { '_' }).take(200).collect();
    let colored = points.iter().any(|(_, c)| c.is_some());
    let d = s.doc_mut()?;
    // A taken name gets a number.
    let mut name = wanted.clone();
    let mut i = 2;
    while d.block(&name).is_some() && i < 100_000 {
        name = format!("{wanted}_{i}");
        i += 1;
    }
    let mut block = Block::new(&name);
    block.description = format!("Point cloud from {path}");
    let layer = d.header.str("CLAYER", "0");
    for (pt, rgb) in &points {
        let mut e = Entity::new(d.new_handle(), EntityKind::Point(Point { p: *pt, angle: 0.0 }));
        e.common.layer = "0".into();
        if let Some(c) = rgb {
            e.common.color = Color::True(*c);
        }
        block.entities.push(e);
    }
    d.blocks.insert(name.clone(), Arc::new(block));
    let ins = Insert {
        block: name.clone(),
        insert: at.to3(0.0),
        scale: Vec3::new(scale, scale, scale),
        rotation: 0.0,
        attribs: Vec::new(),
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
        clip: None,
    };
    let h = d.new_handle();
    let mut e = Entity::new(h, EntityKind::Insert(ins));
    e.common.layer = layer;
    let space = s.space();
    if let Some(store) = s.doc_mut()?.space_mut(&space) {
        store.push(e);
    }
    let kept = points.len();
    let message = if kept < total {
        format!("Point cloud \"{name}\": {kept} of {total} points (thinned).")
    } else {
        format!("Point cloud \"{name}\": {kept} points.")
    };
    Ok(json!({ "handle": h.hex(), "block": name, "points": kept, "total": total, "colored": colored, "message": message }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::file::base64_encode;

    fn las(points: &[(i32, i32, i32, [u16; 3])]) -> Vec<u8> {
        let mut b = vec![0u8; 227];
        b[0..4].copy_from_slice(b"LASF");
        b[24] = 1;
        b[25] = 2;
        b[94..96].copy_from_slice(&227u16.to_le_bytes());
        b[96..100].copy_from_slice(&227u32.to_le_bytes());
        b[104] = 2;
        b[105..107].copy_from_slice(&26u16.to_le_bytes());
        b[107..111].copy_from_slice(&(points.len() as u32).to_le_bytes());
        for (i, s) in [0.01f64, 0.01, 0.01].iter().enumerate() {
            b[131 + i * 8..139 + i * 8].copy_from_slice(&s.to_le_bytes());
        }
        b[155..163].copy_from_slice(&1000.0f64.to_le_bytes());
        for (x, y, z, c) in points {
            let mut r = vec![0u8; 26];
            r[0..4].copy_from_slice(&x.to_le_bytes());
            r[4..8].copy_from_slice(&y.to_le_bytes());
            r[8..12].copy_from_slice(&z.to_le_bytes());
            for (k, v) in c.iter().enumerate() {
                r[20 + k * 2..22 + k * 2].copy_from_slice(&v.to_le_bytes());
            }
            b.extend(r);
        }
        b
    }

    #[test]
    fn xyz_points_are_read_and_thinned() {
        let text = "# header\n0 0 0 255 0 0\n1,2,3\nbad line\n2;4;6;10;0;255;0\nnan 1 2\n";
        let (pts, total) = parse_xyz(text, 10);
        assert_eq!(total, 3);
        assert_eq!(pts[0].1, Some(Rgb(255, 0, 0)));
        assert_eq!(pts[1], (Vec3::new(1.0, 2.0, 3.0), None));
        assert_eq!(pts[2].1, Some(Rgb(0, 255, 0)));
        let many: String = (0..1000).map(|i| format!("{i} 0 0\n")).collect();
        let (pts, total) = parse_xyz(&many, 100);
        assert_eq!((pts.len(), total), (100, 1000));
    }

    #[test]
    fn las_points_and_colours() {
        let b = las(&[(100, 200, 300, [65535, 0, 0]), (0, 0, 0, [0, 65535, 0])]);
        let (pts, total) = parse_las(&b, 10).unwrap();
        assert_eq!(total, 2);
        assert_eq!(pts[0].0, Vec3::new(1001.0, 2.0, 3.0));
        assert_eq!(pts[0].1, Some(Rgb(255, 0, 0)));
        // A record count larger than the file holds reads only what is there.
        let mut lying = b.clone();
        lying[107..111].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse_las(&lying, 10).unwrap().1, 2);
        // Damaged and unsupported files are refused.
        assert!(parse_las(b"LASF", 10).is_err());
        let mut laz = b.clone();
        laz[104] |= 0x80;
        assert!(parse_las(&laz, 10).is_err());
        let mut tiny = b;
        tiny[105..107].copy_from_slice(&4u16.to_le_bytes());
        assert!(parse_las(&tiny, 10).is_err());
    }

    #[test]
    fn a_cloud_is_one_block_reference() {
        let mut s = Session::new();
        let data = base64_encode(b"0 0 0\n10 0 0\n10 10 5\n");
        let r = s.execute(ID, &json!({"path": "scan.xyz", "data": data, "at": [5, 5]})).unwrap();
        assert_eq!(r["points"], 3);
        assert_eq!(r["block"], "PointCloud_scan");
        let r2 = s.execute(ID, &json!({"path": "scan.xyz", "data": data})).unwrap();
        assert_eq!(r2["block"], "PointCloud_scan_2");
        assert_eq!(s.doc().unwrap().model.len(), 2);
        for bad in [
            json!({"path": "a.rcp"}),
            json!({"path": "a.xyz", "data": base64_encode(b"no points")}),
            json!({"path": "a.xyz", "data": data, "maxPoints": 0}),
        ] {
            assert!(s.execute(ID, &bad).is_err(), "{bad}");
        }
    }
}
