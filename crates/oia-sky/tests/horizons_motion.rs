//! Numerical checks for two-body propagation and light-time correction.
//!
//! Horizons responses are independent external references, not OIA's runtime
//! ephemeris. Elements fitted to the first geometric row are a test fixture;
//! only the second row checks short-horizon propagation. This does not establish
//! absolute or long-range planetary/lunar accuracy.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;

use oia_sky::motion::{
    CartesianState, CenterId, ElementProvenance, EllipticalElements, KeplerOrbit,
    KeplerianParameters, LightTimeError, LightTimeLimits, MotionError, PositionAu,
    PropagationLimits, ReferenceFrame, SPEED_OF_LIGHT_AU_PER_DAY, TdbEpoch, VelocityAuPerDay,
    light_time_corrected_position,
};

const AU_KM: f64 = 149_597_870.700;
const TAU: f64 = 2.0 * std::f64::consts::PI;

#[derive(Clone, Copy, Debug)]
struct VectorRow {
    jd_tdb: f64,
    position: [f64; 3],
    velocity: [f64; 3],
    light_time_days: f64,
    range_au: f64,
}

impl VectorRow {
    fn epoch(self) -> TdbEpoch {
        TdbEpoch::new(self.jd_tdb).unwrap()
    }
}

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

fn vector_rows(name: &str, correction: &str) -> Vec<VectorRow> {
    let text = fixture_result(name);
    assert!(text.contains("Output units    : AU-D"), "{name}: units");
    assert!(
        text.contains("Reference frame : Ecliptic of J2000.0"),
        "{name}: frame"
    );
    assert!(
        text.contains(&format!("Output type     : {correction} cartesian states")),
        "{name}: correction"
    );
    let soe = text.find("$$SOE").expect("$$SOE");
    let eoe = text.find("$$EOE").expect("$$EOE");
    let header = text[..soe]
        .lines()
        .rev()
        .find(|line| line.contains("JDTDB"))
        .expect("vector header");
    let columns: Vec<String> = header.split(',').map(|c| c.trim().to_owned()).collect();
    assert_eq!(columns.len(), 12, "{name}: vector columns");
    assert_eq!(columns[0], "JDTDB");
    assert_eq!(columns[2], "X");
    assert_eq!(columns[5], "VX");
    assert_eq!(columns[8], "LT");
    assert_eq!(columns[11], "", "{name}: trailing CSV comma");

    let rows: Vec<VectorRow> = text[soe + 5..eoe]
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let fields: Vec<String> = line.split(',').map(|f| f.trim().to_owned()).collect();
            assert_eq!(fields.len(), columns.len(), "{name}: field count");
            let value = |index: usize| fields[index].parse::<f64>().unwrap();
            VectorRow {
                jd_tdb: value(0),
                position: [value(2), value(3), value(4)],
                velocity: [value(5), value(6), value(7)],
                light_time_days: value(8),
                range_au: value(9),
            }
        })
        .collect();
    assert_eq!(rows.len(), 2, "{name}: retained row count");
    rows
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn mu_au_day(gm_km3_s2: f64) -> f64 {
    gm_km3_s2 * 86_400.0_f64.powi(2) / AU_KM.powi(3)
}

fn fit_orbit<'a>(name: &'a str, center_name: &'a str, row: VectorRow, mu: f64) -> KeplerOrbit<'a> {
    let r = row.position;
    let v = row.velocity;
    let range = norm(r);
    let speed2 = dot(v, v);
    let angular_momentum = cross(r, v);
    let h = norm(angular_momentum);
    let r_dot_v = dot(r, v);
    let eccentricity_vector = [
        ((speed2 - mu / range) * r[0] - r_dot_v * v[0]) / mu,
        ((speed2 - mu / range) * r[1] - r_dot_v * v[1]) / mu,
        ((speed2 - mu / range) * r[2] - r_dot_v * v[2]) / mu,
    ];
    let eccentricity = norm(eccentricity_vector);
    let semi_major_axis = 1.0 / (2.0 / range - speed2 / mu);
    let inclination = (angular_momentum[2] / h).clamp(-1.0, 1.0).acos();
    let node = [-angular_momentum[1], angular_momentum[0], 0.0];
    let node_norm = norm(node);
    assert!(
        node_norm > 0.0 && eccentricity > 0.0,
        "test fit is oriented"
    );
    let ascending_node = node[1].atan2(node[0]).rem_euclid(TAU);
    let argument_of_periapsis = {
        let cosine = (dot(node, eccentricity_vector) / (node_norm * eccentricity)).clamp(-1.0, 1.0);
        let angle = cosine.acos();
        if eccentricity_vector[2] < 0.0 {
            TAU - angle
        } else {
            angle
        }
    };
    let true_anomaly = {
        let cosine = (dot(eccentricity_vector, r) / (eccentricity * range)).clamp(-1.0, 1.0);
        let angle = cosine.acos();
        if r_dot_v < 0.0 { TAU - angle } else { angle }
    };
    let eccentric_anomaly = 2.0
        * ((1.0 - eccentricity).sqrt() * (true_anomaly / 2.0).sin())
            .atan2((1.0 + eccentricity).sqrt() * (true_anomaly / 2.0).cos());
    let mean_anomaly = (eccentric_anomaly - eccentricity * eccentric_anomaly.sin()).rem_euclid(TAU);

    let provenance = ElementProvenance::new(
        "JPL Horizons geometric state; test-only osculating two-body fit",
        name,
    )
    .unwrap();
    let center = CenterId::new(center_name).unwrap();
    let elements = EllipticalElements::new(
        provenance,
        center,
        row.epoch(),
        KeplerianParameters {
            semi_major_axis_au: semi_major_axis,
            eccentricity,
            inclination_deg: inclination.to_degrees(),
            ascending_node_deg: ascending_node.to_degrees(),
            argument_of_periapsis_deg: argument_of_periapsis.to_degrees(),
            mean_anomaly_at_epoch_deg: mean_anomaly.to_degrees(),
            gravitational_parameter_au3_day2: mu,
        },
    )
    .unwrap();
    KeplerOrbit::new(elements).unwrap()
}

fn zero_state<'a>(epoch: TdbEpoch, center: CenterId<'a>) -> CartesianState<'a> {
    CartesianState::new_geometric(
        epoch,
        ReferenceFrame::EclipticOfJ2000,
        center,
        PositionAu::new(0.0, 0.0, 0.0).unwrap(),
        VelocityAuPerDay::new(0.0, 0.0, 0.0).unwrap(),
    )
}

fn distance_au(a: PositionAu, b: [f64; 3]) -> f64 {
    ((a.x() - b[0]).powi(2) + (a.y() - b[1]).powi(2) + (a.z() - b[2]).powi(2)).sqrt()
}

#[test]
fn circular_orbit_preserves_units_orientation_and_provenance() {
    let epoch = TdbEpoch::new(2_451_545.0).unwrap();
    let provenance = ElementProvenance::new("analytic fixture", "circular-v1").unwrap();
    let center = CenterId::new("analytic center").unwrap();
    let mu = 4.0 * std::f64::consts::PI.powi(2);
    let orbit = KeplerOrbit::new(
        EllipticalElements::new(
            provenance,
            center,
            epoch,
            KeplerianParameters {
                semi_major_axis_au: 1.0,
                eccentricity: 0.0,
                inclination_deg: 0.0,
                ascending_node_deg: 0.0,
                argument_of_periapsis_deg: 0.0,
                mean_anomaly_at_epoch_deg: 0.0,
                gravitational_parameter_au3_day2: mu,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let later = TdbEpoch::new(epoch.jd_tdb() + 0.25).unwrap();
    let propagated = orbit
        .state_at(later, PropagationLimits::new(20, 1.0e-14).unwrap())
        .unwrap();
    let state = propagated.state();
    assert_eq!(state.epoch(), later);
    assert_eq!(state.center(), center);
    assert_eq!(state.frame(), ReferenceFrame::EclipticOfJ2000);
    assert_eq!(propagated.provenance(), provenance);
    assert_eq!(propagated.element_epoch(), epoch);
    assert_eq!(propagated.element_age_days(), 0.25);
    assert!(propagated.kepler_report().residual_rad <= 1.0e-14);
    assert!(distance_au(state.position(), [0.0, 1.0, 0.0]) < 1.0e-12);
    assert!((state.velocity().x() + 2.0 * std::f64::consts::PI).abs() < 1.0e-12);
    assert!(state.velocity().y().abs() < 1.0e-12);
    assert!(state.velocity().z().abs() < 1.0e-12);
}

#[test]
fn stationary_light_time_has_a_retarded_epoch_and_bounded_failure() {
    let reception = TdbEpoch::new(2_460_409.25).unwrap();
    let center = CenterId::new("analytic center").unwrap();
    let target_position = PositionAu::new(1.0, 0.0, 0.0).unwrap();
    let target = |epoch| -> Result<CartesianState<'_>, MotionError> {
        Ok(CartesianState::new_geometric(
            epoch,
            ReferenceFrame::EclipticOfJ2000,
            center,
            target_position,
            VelocityAuPerDay::new(0.0, 0.0, 0.0).unwrap(),
        ))
    };
    let observer =
        |epoch| -> Result<CartesianState<'_>, MotionError> { Ok(zero_state(epoch, center)) };
    let corrected = light_time_corrected_position(
        reception,
        target,
        observer,
        LightTimeLimits::new(4, 1.0e-14).unwrap(),
    )
    .unwrap();
    assert_eq!(corrected.range_au(), 1.0);
    assert!((corrected.light_time_days() - 1.0 / SPEED_OF_LIGHT_AU_PER_DAY).abs() < 1.0e-14);
    assert!(
        (reception.days_since(corrected.emission()) - corrected.light_time_days()).abs() < 1.0e-9
    );
    assert_eq!(corrected.residual_days(), 0.0);
    assert_eq!(corrected.tolerance_days(), 1.0e-14);

    let target = |epoch| -> Result<CartesianState<'_>, MotionError> {
        Ok(CartesianState::new_geometric(
            epoch,
            ReferenceFrame::EclipticOfJ2000,
            center,
            target_position,
            VelocityAuPerDay::new(0.0, 0.0, 0.0).unwrap(),
        ))
    };
    let observer =
        |epoch| -> Result<CartesianState<'_>, MotionError> { Ok(zero_state(epoch, center)) };
    let error = light_time_corrected_position(
        reception,
        target,
        observer,
        LightTimeLimits::new(1, 1.0e-14).unwrap(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        LightTimeError::Motion(MotionError::LightTimeDidNotConverge { iterations: 1, .. })
    ));
}

#[test]
fn one_hour_two_body_propagation_matches_independent_horizons_rows() {
    const SUN_GM: f64 = 132_712_440_041.939_38;
    const EARTH_GM: f64 = 398_600.435_436;
    const MARS_GM: f64 = 42_828.375_214;
    const JUPITER_GM: f64 = 126_712_764.8;
    const MOON_GM: f64 = 4_902.800_066;
    let cases = [
        ("vec-helio-earth-geometric", "Sun", SUN_GM + EARTH_GM),
        ("vec-helio-mars-geometric", "Sun", SUN_GM + MARS_GM),
        ("vec-helio-jupiter-geometric", "Sun", SUN_GM + JUPITER_GM),
        ("vec-geo-moon-geometric", "Earth", EARTH_GM + MOON_GM),
    ];
    let limits = PropagationLimits::new(50, 1.0e-13).unwrap();
    for (name, center, gm) in cases {
        let rows = vector_rows(name, "GEOMETRIC");
        let orbit = fit_orbit(name, center, rows[0], mu_au_day(gm));
        let propagated = orbit.state_at(rows[1].epoch(), limits).unwrap();
        let error_km = distance_au(propagated.state().position(), rows[1].position) * AU_KM;
        let age_days = propagated.element_age_days();
        eprintln!("two-body {name}: age {age_days:.9} d, residual {error_km:.6} km");
        // A deliberately coarse bound for numerical wiring, not an accuracy
        // claim: only one hour is checked after fitting elements to row zero.
        assert!(error_km <= 50.0, "{name}: {error_km:.6} km");
        assert_eq!(age_days, rows[1].epoch().days_since(rows[0].epoch()));
        assert_eq!(propagated.provenance().version(), name);
    }
}

#[test]
fn light_time_matches_separately_retained_horizons_mars_rows() {
    const SUN_GM: f64 = 132_712_440_041.939_38;
    const MARS_GM: f64 = 42_828.375_214;
    let geometric = vector_rows("vec-helio-mars-geometric", "GEOMETRIC");
    let corrected_reference = vector_rows("vec-helio-mars-lt", "LT CORRECTED");
    let orbit = fit_orbit(
        "vec-helio-mars-geometric",
        "Sun",
        geometric[0],
        mu_au_day(SUN_GM + MARS_GM),
    );
    let propagation_limits = PropagationLimits::new(50, 1.0e-13).unwrap();

    for (geometric_row, expected) in geometric.iter().zip(corrected_reference.iter()) {
        assert_eq!(geometric_row.jd_tdb, expected.jd_tdb);
        let center = CenterId::new("Sun").unwrap();
        let target = |epoch| -> Result<CartesianState<'_>, MotionError> {
            orbit
                .state_at(epoch, propagation_limits)
                .map(|state| state.state())
        };
        let observer =
            |epoch| -> Result<CartesianState<'_>, MotionError> { Ok(zero_state(epoch, center)) };
        let corrected = light_time_corrected_position(
            expected.epoch(),
            target,
            observer,
            LightTimeLimits::new(20, 1.0e-13).unwrap(),
        )
        .unwrap();
        let position_error_km =
            distance_au(corrected.observer_to_target_au(), expected.position) * AU_KM;
        let light_time_error_days = (corrected.light_time_days() - expected.light_time_days).abs();
        let range_error_km = (corrected.range_au() - expected.range_au).abs() * AU_KM;
        let correction_km = norm([
            expected.position[0] - geometric_row.position[0],
            expected.position[1] - geometric_row.position[1],
            expected.position[2] - geometric_row.position[2],
        ]) * AU_KM;
        eprintln!(
            "light-time Mars {}: correction {correction_km:.3} km, position residual {position_error_km:.6} km, LT residual {light_time_error_days:.3e} d, iterations {}",
            expected.jd_tdb,
            corrected.iterations()
        );
        assert!(correction_km > 1_000.0, "correction must be non-trivial");
        assert!(position_error_km <= 50.0, "{position_error_km:.6} km");
        assert!(range_error_km <= 50.0, "{range_error_km:.6} km");
        assert!(light_time_error_days <= 2.0e-9);
        assert!(corrected.residual_days() <= corrected.tolerance_days());
        assert!(
            (expected.epoch().days_since(corrected.emission()) - corrected.light_time_days()).abs()
                < 1.0e-9
        );
    }
}

#[test]
fn invalid_domains_and_exhausted_kepler_budget_are_typed_errors() {
    let epoch = TdbEpoch::new(2_451_545.0).unwrap();
    let provenance = ElementProvenance::new("analytic fixture", "invalid-v1").unwrap();
    let center = CenterId::new("analytic center").unwrap();
    let parameters = KeplerianParameters {
        semi_major_axis_au: 1.0,
        eccentricity: 0.9,
        inclination_deg: 0.0,
        ascending_node_deg: 0.0,
        argument_of_periapsis_deg: 0.0,
        mean_anomaly_at_epoch_deg: 60.0,
        gravitational_parameter_au3_day2: 1.0,
    };
    let orbit =
        KeplerOrbit::new(EllipticalElements::new(provenance, center, epoch, parameters).unwrap())
            .unwrap();
    assert!(matches!(
        orbit.state_at(epoch, PropagationLimits::new(1, 1.0e-14).unwrap()),
        Err(MotionError::KeplerDidNotConverge { iterations: 1, .. })
    ));

    let mut non_elliptical = parameters;
    non_elliptical.eccentricity = 1.0;
    assert!(EllipticalElements::new(provenance, center, epoch, non_elliptical).is_err());
    assert!(PositionAu::new(f64::NAN, 0.0, 0.0).is_err());
    assert!(VelocityAuPerDay::new(0.0, f64::INFINITY, 0.0).is_err());
    assert!(TdbEpoch::new(f64::NAN).is_err());
    assert!(PropagationLimits::new(0, 1.0e-12).is_err());
    assert!(LightTimeLimits::new(0, 1.0e-12).is_err());
    assert!(LightTimeLimits::new(10, f64::NAN).is_err());
    assert!(CenterId::new("").is_err());
    assert!(ElementProvenance::new("source", "bad\nversion").is_err());
}
