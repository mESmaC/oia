//! Compares the kernel with unmodified JPL Horizons responses in
//! `fixtures/sky/originals` (an independent reference, not an OIA ephemeris).
//!
//! Tolerances, justified in `docs/implementation/sky-observer-geometry.md`:
//! - horizontal transform: 1.0″ (truncated nutation ≤ 0.46″, neglected polar
//!   motion ≲ 0.5″, UT1 − UTC for 2024 rows treated as zero);
//! - parallax and diurnal aberration: 0.1″;
//! - refraction above the model domain: 1.0″ for the whole chain, 0.5″ for the
//!   refraction amount alone.
//!
//! TT is taken as UT + (TDB − UT) from each row; |TT − TDB| < 2 ms, which is
//! negligible for the nutation arguments.

// Test-only: a malformed or missing reference fixture must fail the test, so
// panicking accessors are the assertion. The library keeps these lints denied.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::HashMap;
use std::path::PathBuf;

use oia_sky::{
    ApparentPlace, Atmosphere, GeodeticSite, Instant, Refraction, SkyError, TargetDistance,
    earth_rotation, horizontal, observe, refract, topocentric_place,
};

const AU_KM: f64 = 149_597_870.700;
const DATE: &str = "Date__(UT)__HR:MN:SS";

type Row = HashMap<String, String>;

fn fixture_result(name: &str) -> String {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "..",
        "fixtures",
        "sky",
        "originals",
    ]
    .iter()
    .collect::<PathBuf>()
    .join(format!("{name}.json"));
    let raw = std::fs::read_to_string(&path).expect("fixture is readable");
    // Minimal JSON string decoding for the `result` member; test-only.
    let start = raw.find("\"result\":\"").expect("result member") + "\"result\":\"".len();
    let mut out = String::new();
    let mut chars = raw[start..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return out,
            '\\' => match chars.next().expect("escape") {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    let code = u32::from_str_radix(&hex, 16).expect("hex escape");
                    out.push(char::from_u32(code).expect("scalar"));
                }
                other => out.push(other),
            },
            other => out.push(other),
        }
    }
    panic!("unterminated result string in {name}");
}

fn rows(name: &str) -> Vec<Row> {
    let text = fixture_result(name);
    let soe = text.find("$$SOE").expect("$$SOE");
    let eoe = text.find("$$EOE").expect("$$EOE");
    let header = text[..soe]
        .lines()
        .rev()
        .find(|l| l.contains(DATE))
        .expect("column header");
    let cols: Vec<String> = header.split(',').map(|c| c.trim().to_owned()).collect();
    text[soe + 5..eoe]
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            cols.iter()
                .cloned()
                .zip(l.split(',').map(|f| f.trim().to_owned()))
                .collect()
        })
        .collect()
}

fn num(row: &Row, col: &str) -> f64 {
    row[col]
        .parse()
        .unwrap_or_else(|_| panic!("{col} = {:?}", row[col]))
}

/// Julian date of a Horizons `YYYY-Mon-DD HH:MM:SS` tag (Gregorian calendar).
fn julian_date(tag: &str) -> f64 {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (date, time) = tag.split_once(' ').expect("date and time");
    let mut d = date.split('-');
    let mut year: i64 = d.next().unwrap().parse().unwrap();
    let mon = d.next().unwrap();
    let mut month = MONTHS.iter().position(|m| *m == mon).unwrap() as i64 + 1;
    let day: f64 = d.next().unwrap().parse().unwrap();
    let hms: Vec<f64> = time.split(':').map(|x| x.parse().unwrap()).collect();
    if month <= 2 {
        year -= 1;
        month += 12;
    }
    let a = year.div_euclid(100);
    let b = 2 - a + a.div_euclid(4);
    (365.25 * (year + 4716) as f64).floor()
        + (30.6001 * (month + 1) as f64).floor()
        + day
        + b as f64
        - 1524.5
        + (hms[0] + hms[1] / 60.0 + hms[2] / 3600.0) / 24.0
}

fn instant(row: &Row) -> Instant {
    let jd = julian_date(&row[DATE]);
    Instant::new(jd, jd + num(row, "TDB-UT") / 86_400.0).expect("valid instant")
}

fn arcsec(deg: f64) -> f64 {
    deg * 3600.0
}

fn wrapped_diff_deg(a: f64, b: f64) -> f64 {
    (a - b + 180.0).rem_euclid(360.0) - 180.0
}

#[test]
fn horizontal_transform_matches_horizons() {
    // Topocentric apparent RA/Dec -> Az/El isolates sidereal time and the
    // horizontal rotation. The 1952 rows use UT1 time tags (pre-1962) and
    // refracted RA/Dec, which transform geometrically to refracted Az/El.
    let cases = [
        (
            "topo-dallas-moon-airless",
            (32.7767, -96.7970, 139.0),
            "R.A.__(a-app)",
            "DEC___(a-app)",
            "Azimuth_(a-app)",
            "Elevation_(a-app)",
        ),
        (
            "topo-washington-mars-1952",
            (38.8512, 282.9598, 5.0),
            "R.A._(rfct-app)",
            "DEC_(rfct-app)",
            "Azimuth_(r-app)",
            "Elevation_(r-app)",
        ),
    ];
    let mut checked = 0;
    for (name, (lat, lon, h), ra, dec, az, el) in cases {
        let site = GeodeticSite::new(lat, lon, h).unwrap();
        for row in rows(name) {
            let rotation = earth_rotation(instant(&row));
            let place = oia_sky::TopocentricPlace {
                ra_deg: num(&row, ra),
                dec_deg: num(&row, dec),
                distance_km: None,
            };
            let got = horizontal(place, site, rotation);
            let d_az = arcsec(wrapped_diff_deg(got.azimuth_deg, num(&row, az)))
                * got.elevation_deg.to_radians().cos();
            let d_el = arcsec(got.elevation_deg - num(&row, el));
            assert!(
                d_az.abs() <= 1.0 && d_el.abs() <= 1.0,
                "{name} {}: dAz·cos(el) {d_az:.3}″ dEl {d_el:.3}″",
                row[DATE]
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 18);
}

#[test]
fn topocentric_parallax_matches_horizons() {
    let site = GeodeticSite::new(32.7767, -96.7970, 139.0).unwrap();
    let topo: HashMap<String, Row> = rows("topo-dallas-moon-airless")
        .into_iter()
        .map(|r| (r[DATE].clone(), r))
        .collect();
    let mut checked = 0;
    for geo in rows("geo-moon-eclipse") {
        let Some(expected) = topo.get(&geo[DATE]) else {
            continue;
        };
        let (ra, dec) = (num(&geo, "R.A.__(a-app)"), num(&geo, "DEC___(a-app)"));
        let place = ApparentPlace::new(
            ra,
            dec,
            TargetDistance::Kilometres(num(&geo, "delta") * AU_KM),
        )
        .unwrap();
        let got = topocentric_place(place, site, earth_rotation(instant(&geo))).unwrap();
        let (era, edec) = (
            num(expected, "R.A.__(a-app)"),
            num(expected, "DEC___(a-app)"),
        );
        // The correction being tested is large (horizontal parallax ~0.95°,
        // times cos(elevation ~64°) ≈ 0.44°)...
        assert!(oia_sky::angular_separation_deg(ra, dec, era, edec) > 0.4);
        // ...and is reproduced to a small fraction of an arcsecond.
        let err = arcsec(oia_sky::angular_separation_deg(
            got.ra_deg,
            got.dec_deg,
            era,
            edec,
        ));
        assert!(err <= 0.1, "{}: {err:.4}″", geo[DATE]);
        checked += 1;
    }
    assert_eq!(checked, 3);
}

#[test]
fn refraction_matches_horizons_above_domain_and_is_withheld_below() {
    let atmosphere = Atmosphere::conventional_reference();
    let geo_sun: HashMap<String, Row> = rows("geo-sun-eclipse")
        .into_iter()
        .map(|r| (r[DATE].clone(), r))
        .collect();
    let (mut applied, mut withheld) = (0, 0);
    // Full chain from the geocentric apparent Sun for observers either side of
    // the antimeridian, through sunrise.
    for (name, lon) in [
        ("topo-dateline-east-sun", 179.9),
        ("topo-dateline-west-sun", 180.1),
    ] {
        let site = GeodeticSite::new(-16.8, lon, 0.0).unwrap();
        for row in rows(name) {
            let geo = &geo_sun[&row[DATE]];
            let place = ApparentPlace::new(
                num(geo, "R.A.__(a-app)"),
                num(geo, "DEC___(a-app)"),
                TargetDistance::Kilometres(num(geo, "delta") * AU_KM),
            )
            .unwrap();
            let got = observe(place, site, instant(geo), Some(atmosphere)).unwrap();
            let d_az = arcsec(wrapped_diff_deg(
                got.airless.azimuth_deg,
                num(&row, "Azimuth_(r-app)"),
            ));
            assert!(d_az.abs() <= 1.0, "{name} {}: dAz {d_az:.3}″", row[DATE]);
            match got.refraction {
                Some(Refraction::Applied { elevation_deg, .. }) => {
                    let d_el = arcsec(elevation_deg - num(&row, "Elevation_(r-app)"));
                    assert!(d_el.abs() <= 1.0, "{name} {}: dEl {d_el:.3}″", row[DATE]);
                    applied += 1;
                }
                // Horizons still prints a refracted value here (a constant
                // offset); the kernel must not produce one.
                Some(Refraction::BelowModelDomain {
                    airless_elevation_deg,
                }) => {
                    assert!(airless_elevation_deg < -1.0);
                    withheld += 1;
                }
                None => panic!("refraction was requested"),
            }
        }
    }
    assert_eq!((applied, withheld), (4, 6));

    // Refraction amount alone at ~64° elevation (airless versus refracted files).
    for (airless, refracted) in rows("topo-dallas-sun-airless")
        .iter()
        .zip(rows("topo-dallas-sun-refracted").iter())
    {
        assert_eq!(airless[DATE], refracted[DATE]);
        let h = num(airless, "Elevation_(a-app)");
        let expected = arcsec(num(refracted, "Elevation_(r-app)") - h);
        let Refraction::Applied {
            refraction_arcsec, ..
        } = refract(h, atmosphere).unwrap()
        else {
            panic!("above domain");
        };
        assert!(
            (refraction_arcsec - expected).abs() <= 0.5,
            "{}: {refraction_arcsec:.3}″ vs {expected:.3}″",
            airless[DATE]
        );
    }
}

#[test]
fn invalid_inputs_are_rejected_before_computation() {
    assert_eq!(
        GeodeticSite::new(f64::NAN, 0.0, 0.0),
        Err(SkyError::NonFinite(oia_sky::Field::Latitude))
    );
    assert!(matches!(
        GeodeticSite::new(91.0, 0.0, 0.0),
        Err(SkyError::OutOfRange { .. })
    ));
    // Outside 1900–2100, and TT/UT1 swapped (TT − UT1 = −69 s).
    assert!(Instant::new(2_400_000.5, 2_400_000.5).is_err());
    let jd = 2_460_409.25;
    assert!(matches!(
        Instant::new(jd + 69.0 / 86_400.0, jd),
        Err(SkyError::OutOfRange {
            field: oia_sky::Field::TtMinusUt1,
            ..
        })
    ));
    assert!(ApparentPlace::new(360.0, 0.0, TargetDistance::ParallaxNegligible).is_err());
    assert!(ApparentPlace::new(10.0, 0.0, TargetDistance::Kilometres(6_000.0)).is_err());
    assert!(ApparentPlace::new(10.0, 0.0, TargetDistance::Kilometres(f64::INFINITY)).is_err());
    assert!(Atmosphere::new(-1.0, 10.0).is_err());
    assert!(refract(f64::NAN, Atmosphere::conventional_reference()).is_err());
}
