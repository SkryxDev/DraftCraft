//! GEOGRAPHICLOCATION and GEOREMOVE: where on Earth the drawing is. The drawing's origin is at
//! the given latitude and longitude, and the north direction is the angle from the drawing's
//! Y axis to north. They are kept in the published DXF header variables `$LATITUDE`,
//! `$LONGITUDE` and `$NORTHDIRECTION`.
//!
//! With a location set, a drawing point converts to latitude and longitude (`point`) and back
//! (`latitude`/`longitude` with `toPoint`), on a local plane tangent to the WGS 84 ellipsoid at
//! the location: accurate to centimetres within a few kilometres, which is what a site drawing
//! spans.

use draftcraft_doc::HVal;
use serde_json::{Value, json};

use crate::cmd::{CommandSpec, bad, point_value};
use crate::{Result, Session};

/// WGS 84 semi-major axis (metres) and first eccentricity squared.
const WGS84_A: f64 = 6_378_137.0;
const WGS84_E2: f64 = 0.006_694_379_990_14;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("geographiclocation", "Geographic Location...", run_geo).menu(&["Tools", "Geographic Location..."]).alias(&["geo"]).params(
            "{latitude?: -90..90, longitude?: -180..180 (degrees, WGS 84; sets the location of the drawing's origin), north?: \
                 degrees (the angle from the drawing's Y axis to north, counter-clockwise), point?: [x,y] (convert this drawing point to \
                 latitude/longitude), toPoint?: bool (with latitude/longitude: convert them to a drawing point instead of setting the \
                 location)} → {latitude, longitude, north, units} | {point, latitude, longitude}",
        ),
        CommandSpec::new("georemove", "Remove Location", run_georemove).menu(&["Tools", "Remove Location"]).params("{}"),
    ]
}

/// Metres per drawing unit for INSUNITS (unitless drawings count as metres).
fn metres_per_unit(insunits: i64) -> f64 {
    match insunits {
        1 => 0.0254,
        2 => 0.3048,
        3 => 1609.344,
        4 => 0.001,
        5 => 0.01,
        7 => 1000.0,
        8 => 2.54e-8,
        9 => 2.54e-5,
        10 => 0.9144,
        14 => 0.1,
        21 => 1200.0 / 3937.0,
        _ => 1.0,
    }
}

/// The location: (latitude, longitude, north in radians), if one is set.
fn location(s: &Session) -> Result<Option<(f64, f64, f64)>> {
    let h = &s.doc()?.header;
    let (lat, lon) = (h.get("LATITUDE").and_then(HVal::as_f64), h.get("LONGITUDE").and_then(HVal::as_f64));
    let north = h.get("NORTHDIRECTION").and_then(HVal::as_f64).filter(|n| n.is_finite()).unwrap_or(0.0);
    Ok(match (lat, lon) {
        (Some(lat), Some(lon)) if lat.is_finite() && lon.is_finite() => Some((lat, lon, north)),
        _ => None,
    })
}

/// Radii of curvature at `lat` (degrees): (meridian, prime vertical), metres.
fn radii(lat: f64) -> (f64, f64) {
    let s = lat.to_radians().sin();
    let w = (1.0 - WGS84_E2 * s * s).sqrt();
    (WGS84_A * (1.0 - WGS84_E2) / (w * w * w), WGS84_A / w)
}

/// A drawing point's latitude and longitude.
fn to_geo(loc: (f64, f64, f64), unit_m: f64, x: f64, y: f64) -> (f64, f64) {
    let (lat0, lon0, north) = loc;
    // East/north metres: the drawing turned so that north is up.
    let (sn, cn) = north.sin_cos();
    let (e, n) = ((x * cn + y * sn) * unit_m, (-x * sn + y * cn) * unit_m);
    let (m, pv) = radii(lat0);
    let lat = lat0 + (n / m).to_degrees();
    let lon = lon0 + (e / (pv * lat0.to_radians().cos().max(1e-9))).to_degrees();
    (lat, lon)
}

/// A latitude and longitude's drawing point.
fn to_point(loc: (f64, f64, f64), unit_m: f64, lat: f64, lon: f64) -> (f64, f64) {
    let (lat0, lon0, north) = loc;
    let (m, pv) = radii(lat0);
    let n = (lat - lat0).to_radians() * m;
    let e = (lon - lon0).to_radians() * pv * lat0.to_radians().cos();
    let (sn, cn) = north.sin_cos();
    ((e * cn - n * sn) / unit_m, (e * sn + n * cn) / unit_m)
}

fn degrees(p: &Value, k: &str, limit: f64) -> Result<Option<f64>> {
    match p.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|x| x.is_finite() && x.abs() <= limit)
            .map(Some)
            .ok_or_else(|| bad("geographiclocation", format!("`{k}` must be a number of degrees within ±{limit}"))),
    }
}

fn run_geo(s: &mut Session, p: &Value) -> Result<Value> {
    let unit_m = metres_per_unit(s.doc()?.header.i64("INSUNITS", 0));
    let (lat, lon) = (degrees(p, "latitude", 90.0)?, degrees(p, "longitude", 180.0)?);
    let north = degrees(p, "north", 360.0)?;
    if let Some(pt) = p.get("point") {
        let pt = point_value(pt).ok_or_else(|| bad("geographiclocation", "`point` must be [x, y]"))?;
        let loc = location(s)?.ok_or_else(|| bad("geographiclocation", "the drawing has no geographic location"))?;
        let (la, lo) = to_geo(loc, unit_m, pt.x, pt.y);
        return Ok(json!({ "point": [pt.x, pt.y], "latitude": la, "longitude": lo }));
    }
    if p.get("toPoint").and_then(Value::as_bool).unwrap_or(false) {
        let loc = location(s)?.ok_or_else(|| bad("geographiclocation", "the drawing has no geographic location"))?;
        let (la, lo) = lat.zip(lon).ok_or_else(|| bad("geographiclocation", "`latitude` and `longitude` are the place to convert"))?;
        let (x, y) = to_point(loc, unit_m, la, lo);
        return Ok(json!({ "point": [x, y], "latitude": la, "longitude": lo }));
    }
    if lat.is_some() != lon.is_some() {
        return Err(bad("geographiclocation", "give both `latitude` and `longitude`"));
    }
    if lat.is_some() || north.is_some() {
        let has = location(s)?.is_some();
        if lat.is_none() && !has {
            return Err(bad("geographiclocation", "set `latitude` and `longitude` first"));
        }
        let h = &mut s.doc_mut()?.header;
        if let (Some(la), Some(lo)) = (lat, lon) {
            h.set("LATITUDE", HVal::Real(la));
            h.set("LONGITUDE", HVal::Real(lo));
        }
        if let Some(n) = north {
            h.set("NORTHDIRECTION", HVal::Real(n.to_radians()));
        }
    }
    match location(s)? {
        Some((la, lo, n)) => {
            let message = format!("Location: latitude {la:.6}, longitude {lo:.6}, north {:.2}°.", n.to_degrees());
            Ok(json!({ "latitude": la, "longitude": lo, "north": n.to_degrees(), "unitMetres": unit_m, "message": message }))
        }
        None => Ok(json!({ "latitude": null, "longitude": null, "message": "The drawing has no geographic location." })),
    }
}

fn run_georemove(s: &mut Session, _p: &Value) -> Result<Value> {
    if location(s)?.is_none() {
        return Err(bad("georemove", "the drawing has no geographic location"));
    }
    let h = &mut s.doc_mut()?.header;
    for k in ["LATITUDE", "LONGITUDE", "NORTHDIRECTION"] {
        h.vars.remove(k);
    }
    Ok(json!({ "message": "Geographic location removed." }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_location_converts_both_ways() {
        let mut s = Session::new();
        assert!(s.execute("geographiclocation", &json!({"point": [0, 0]})).is_err(), "no location yet");
        s.doc_mut().unwrap().header.set_i64("INSUNITS", 6);
        s.execute("geographiclocation", &json!({"latitude": 45.4642, "longitude": 9.19, "north": 30})).unwrap();
        let r = s.execute("geographiclocation", &json!({"point": [100, 250]})).unwrap();
        let (la, lo) = (r["latitude"].as_f64().unwrap(), r["longitude"].as_f64().unwrap());
        let back = s.execute("geographiclocation", &json!({"latitude": la, "longitude": lo, "toPoint": true})).unwrap();
        let (x, y) = (back["point"][0].as_f64().unwrap(), back["point"][1].as_f64().unwrap());
        assert!((x - 100.0).abs() < 1e-6 && (y - 250.0).abs() < 1e-6, "{back}");
        // One kilometre north on an unrotated drawing is about 0.009° of latitude.
        s.execute("geographiclocation", &json!({"north": 0})).unwrap();
        let r = s.execute("geographiclocation", &json!({"point": [0, 1000]})).unwrap();
        assert!((r["latitude"].as_f64().unwrap() - 45.4642 - 0.009).abs() < 0.0005, "{r}");
        assert!(s.execute("geographiclocation", &json!({"latitude": 91, "longitude": 0})).is_err());
        assert!(s.execute("geographiclocation", &json!({"latitude": 10})).is_err());
        s.execute("georemove", &json!({})).unwrap();
        assert!(s.execute("georemove", &json!({})).is_err());
    }
}
