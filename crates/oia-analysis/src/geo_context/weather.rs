//! Atmospheric and meteorological context types (`GEO-02`, `GEO-05`).

use crate::geo_context::GeoContextError;
use oia_domain::{OpaqueId, VersionId};
use oia_time_geo::{Altitude, GeoPosition, TimeWindow};

/// Classification of a meteorological record as a direct sensor measurement vs. forecast model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeatherReportKind {
    DirectObservation,
    ForecastModel,
}

/// Cloud coverage classification (WMO / ICAO code).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudCoverage {
    Clear,
    Few,
    Scattered,
    Broken,
    Overcast,
}

/// Single atmospheric cloud layer report.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudLayer {
    pub coverage: CloudCoverage,
    pub base_altitude: Altitude,
    pub top_altitude: Option<Altitude>,
}

/// Upper-air vertical atmospheric sounding profile level.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundingLevel {
    pub altitude: Altitude,
    pub temperature_c: f64,
    pub pressure_hpa: f64,
    pub wind_speed_mps: f64,
    pub wind_direction_deg: f64,
}

/// Normalized weather observation or forecast model report.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherRecord {
    pub source_id: OpaqueId,
    pub version_id: VersionId,
    pub station_id: OpaqueId,
    pub station_position: GeoPosition,
    pub kind: WeatherReportKind,
    pub observation_instant_utc_ms: Option<i64>,
    pub forecast_issuance_utc_ms: Option<i64>,
    pub retrieval_instant_utc_ms: i64,
    pub valid_window: TimeWindow,
    pub model_id: Option<OpaqueId>,
    pub temperature_c: Option<f64>,
    pub dew_point_c: Option<f64>,
    pub relative_humidity_pct: Option<f64>,
    pub wind_speed_mps: Option<f64>,
    pub wind_direction_deg: Option<f64>,
    pub wind_gust_mps: Option<f64>,
    pub visibility_m: Option<f64>,
    pub visibility_restriction: Option<String>,
    pub pressure_altimeter_hpa: Option<f64>,
    pub cloud_layers: Vec<CloudLayer>,
    pub sounding_profile: Vec<SoundingLevel>,
    pub raw_text_hash_sha256: [u8; 32],
}

impl WeatherRecord {
    /// Validates structural consistency and temporal integrity.
    pub fn validate(&self) -> Result<(), GeoContextError> {
        self.valid_window.validate()?;
        match self.kind {
            WeatherReportKind::DirectObservation => {
                let obs =
                    self.observation_instant_utc_ms
                        .ok_or(GeoContextError::InvalidWeatherTimes(
                            "direct observation missing observation_instant_utc_ms",
                        ))?;
                if self.forecast_issuance_utc_ms.is_some() {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "direct observation must not specify forecast_issuance_utc_ms",
                    ));
                }
                if self.model_id.is_some() {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "direct observation must not specify model_id",
                    ));
                }
                if obs < self.valid_window.start_utc_ms() || obs > self.valid_window.end_utc_ms() {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "observation_instant_utc_ms falls outside valid_window",
                    ));
                }
                if self.retrieval_instant_utc_ms < obs {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "retrieval_instant_utc_ms must be >= observation_instant_utc_ms",
                    ));
                }
            }
            WeatherReportKind::ForecastModel => {
                let iss =
                    self.forecast_issuance_utc_ms
                        .ok_or(GeoContextError::InvalidWeatherTimes(
                            "forecast model missing forecast_issuance_utc_ms",
                        ))?;
                if self.model_id.is_none() {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "forecast model missing model_id",
                    ));
                }
                if self.observation_instant_utc_ms.is_some() {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "forecast model must not specify observation_instant_utc_ms",
                    ));
                }
                // Permit amended forecasts issued during validity window, but not after expiration
                if iss > self.valid_window.end_utc_ms() {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "forecast_issuance_utc_ms occurs after valid_window end",
                    ));
                }
                if self.retrieval_instant_utc_ms < iss {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "retrieval_instant_utc_ms must be >= forecast_issuance_utc_ms",
                    ));
                }
            }
        }

        if let Some(t) = self.temperature_c
            && !t.is_finite()
        {
            return Err(GeoContextError::NonFiniteValue("temperature_c"));
        }
        if let Some(dp) = self.dew_point_c
            && !dp.is_finite()
        {
            return Err(GeoContextError::NonFiniteValue("dew_point_c"));
        }
        if let Some(rh) = self.relative_humidity_pct
            && (!rh.is_finite() || !(0.0..=100.0).contains(&rh))
        {
            return Err(GeoContextError::NonFiniteValue("relative_humidity_pct"));
        }
        if let Some(ws) = self.wind_speed_mps
            && (!ws.is_finite() || ws < 0.0)
        {
            return Err(GeoContextError::NonFiniteValue("wind_speed_mps"));
        }
        if let Some(wd) = self.wind_direction_deg
            && (!wd.is_finite() || !(0.0..=360.0).contains(&wd))
        {
            return Err(GeoContextError::NonFiniteValue("wind_direction_deg"));
        }
        if let Some(wg) = self.wind_gust_mps
            && (!wg.is_finite() || wg < 0.0)
        {
            return Err(GeoContextError::NonFiniteValue("wind_gust_mps"));
        }
        if let Some(vis) = self.visibility_m
            && (!vis.is_finite() || vis < 0.0)
        {
            return Err(GeoContextError::NonFiniteValue("visibility_m"));
        }
        if let Some(p) = self.pressure_altimeter_hpa
            && (!p.is_finite() || p < 0.0)
        {
            return Err(GeoContextError::NonFiniteValue("pressure_altimeter_hpa"));
        }

        // Validate cloud layers
        for layer in &self.cloud_layers {
            if let Some(ref top) = layer.top_altitude {
                layer.base_altitude.compare_in_same_reference(top)?;
                if layer.base_altitude.meters() > top.meters() {
                    return Err(GeoContextError::InvalidAltitudeBand {
                        min_meters: layer.base_altitude.meters(),
                        max_meters: top.meters(),
                    });
                }
            }
        }

        // Validate sounding profile
        for snd in &self.sounding_profile {
            if !snd.temperature_c.is_finite() {
                return Err(GeoContextError::NonFiniteValue("sounding.temperature_c"));
            }
            if !snd.pressure_hpa.is_finite() || snd.pressure_hpa < 0.0 {
                return Err(GeoContextError::NonFiniteValue("sounding.pressure_hpa"));
            }
            if !snd.wind_speed_mps.is_finite() || snd.wind_speed_mps < 0.0 {
                return Err(GeoContextError::NonFiniteValue("sounding.wind_speed_mps"));
            }
            if !snd.wind_direction_deg.is_finite()
                || !(0.0..=360.0).contains(&snd.wind_direction_deg)
            {
                return Err(GeoContextError::NonFiniteValue(
                    "sounding.wind_direction_deg",
                ));
            }
        }

        Ok(())
    }
}

/// Weather coverage classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeatherCoverageStatus {
    ObservedWeatherAvailable,
    ForecastModelOnly,
    UnobservedCoverageGap,
}

/// Summary of weather conditions reconstructed for the envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherReconstructionSummary {
    pub coverage_status: WeatherCoverageStatus,
    pub matching_records_count: usize,
    pub direct_observations_count: usize,
    pub forecast_models_count: usize,
    pub nearest_observation: Option<WeatherRecord>,
    pub nearest_forecast: Option<WeatherRecord>,
    pub upper_air_soundings_count: usize,
}
