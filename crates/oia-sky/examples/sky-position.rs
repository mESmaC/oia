//! Prints the topocentric and horizontal position of one target for one
//! observer and instant. Read-only; it computes and prints, nothing else.
//!
//! Usage:
//!   sky-position JD_UT1 JD_TT LAT_DEG LON_EAST_DEG HEIGHT_M RA_DEG DEC_DEG DISTANCE [PRESSURE_HPA TEMP_C]
//!
//! DISTANCE is a geocentric distance in km, or `parallax-negligible`. The
//! RA/Dec must be a geocentric apparent place (true equator and equinox of
//! date). Without PRESSURE_HPA and TEMP_C, refraction is not applied.

use std::process::ExitCode;

use oia_sky::{
    ApparentPlace, Atmosphere, GeodeticSite, Instant, MODEL_ID, Refraction, TargetDistance, observe,
};

const USAGE: &str = "usage: sky-position JD_UT1 JD_TT LAT_DEG LON_EAST_DEG HEIGHT_M RA_DEG DEC_DEG DISTANCE_KM|parallax-negligible [PRESSURE_HPA TEMP_C]";

fn number(args: &[String], index: usize, name: &str) -> Result<f64, String> {
    let raw = args
        .get(index)
        .ok_or_else(|| format!("missing {name}\n{USAGE}"))?;
    raw.parse::<f64>()
        .map_err(|_| format!("{name} is not a number: {raw:?}"))
}

fn run(args: &[String]) -> Result<(), String> {
    if !(args.len() == 8 || args.len() == 10) {
        return Err(USAGE.to_owned());
    }
    let instant = Instant::new(number(args, 0, "JD_UT1")?, number(args, 1, "JD_TT")?)
        .map_err(|e| e.to_string())?;
    let site = GeodeticSite::new(
        number(args, 2, "LAT_DEG")?,
        number(args, 3, "LON_EAST_DEG")?,
        number(args, 4, "HEIGHT_M")?,
    )
    .map_err(|e| e.to_string())?;
    let distance = match args.get(7).map(String::as_str) {
        Some("parallax-negligible") => TargetDistance::ParallaxNegligible,
        _ => TargetDistance::Kilometres(number(args, 7, "DISTANCE_KM")?),
    };
    let place = ApparentPlace::new(
        number(args, 5, "RA_DEG")?,
        number(args, 6, "DEC_DEG")?,
        distance,
    )
    .map_err(|e| e.to_string())?;
    let atmosphere = if args.len() == 10 {
        Some(
            Atmosphere::new(number(args, 8, "PRESSURE_HPA")?, number(args, 9, "TEMP_C")?)
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };

    let p = observe(place, site, instant, atmosphere).map_err(|e| e.to_string())?;
    println!("model: {MODEL_ID}");
    println!(
        "inputs: JD_UT1 {:.9}  JD_TT {:.9}  site {:.6}° {:.6}°E {:.1} m (WGS84 ellipsoidal)",
        instant.jd_ut1(),
        instant.jd_tt(),
        site.latitude_deg(),
        site.longitude_east_deg(),
        site.height_m()
    );
    println!(
        "earth rotation: GMST {:.9}°  eq. equinoxes {:.4}\"  GAST {:.9}°",
        p.rotation.gmst_deg, p.rotation.equation_of_equinoxes_arcsec, p.rotation.gast_deg
    );
    match p.topocentric.distance_km {
        Some(km) => println!(
            "topocentric apparent: RA {:.9}°  Dec {:.9}°  distance {km:.3} km",
            p.topocentric.ra_deg, p.topocentric.dec_deg
        ),
        None => println!(
            "topocentric apparent: RA {:.9}°  Dec {:.9}°  (parallax not applied: stated negligible)",
            p.topocentric.ra_deg, p.topocentric.dec_deg
        ),
    }
    println!(
        "airless horizontal: Az {:.9}°  El {:.9}°  hour angle {:.9}°",
        p.airless.azimuth_deg, p.airless.elevation_deg, p.airless.hour_angle_deg
    );
    match (p.refraction, atmosphere) {
        (None, _) | (_, None) => println!("refraction: not requested"),
        (
            Some(Refraction::Applied {
                elevation_deg,
                refraction_arcsec,
            }),
            Some(atm),
        ) => println!(
            "refracted: El {elevation_deg:.9}°  (+{refraction_arcsec:.3}\" at stated {} hPa, {} °C)",
            atm.pressure_hpa(),
            atm.temperature_c()
        ),
        (Some(Refraction::BelowModelDomain { .. }), Some(_)) => println!(
            "refraction: not modelled below {}° airless elevation; no refracted value",
            oia_sky::REFRACTION_MIN_ELEVATION_DEG
        ),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
