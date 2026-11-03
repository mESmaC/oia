//! Registered imagery sources for the Globe instrument (`GEO-02`, ADR 0008 §4 amendment).
//!
//! Every origin the page may load tiles from is listed here with its licence,
//! attribution, capture statement and the date the WMTS capabilities were checked.
//! The page never composes a tile URL from anything that is not in this registry.
//! Dates are validated here so a day outside the published coverage is refused as a
//! documented gap rather than drawn as an empty scene (`AGENTS.md` §2).
//!
//! Above the selected basemap sits a resolution ladder of detail sources: real imagery
//! with a finer source resolution that the renderer draws only once the camera is close
//! enough for it to add detail. Each detail source keeps its own capture statement,
//! licence and coverage rectangle; none of them is a resampled copy of the basemap, and
//! none of them shares the basemap's capture date.

use crate::local_tools::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// NASA GIBS: the basemap origin.
pub const GIBS_ORIGIN: &str = "https://gibs.earthdata.nasa.gov";
/// EOX IT Services: the global detail origin (Sentinel-2 cloudless).
pub const EOX_ORIGIN: &str = "https://tiles.maps.eox.at";
/// USGS The National Map: the United States orthoimagery origin.
pub const USGS_TNM_ORIGIN: &str = "https://basemap.nationalmap.gov";
/// When the GIBS `epsg3857/best` WMTS capabilities were last read for this registry.
pub const CAPABILITIES_CHECKED_ON: &str = "2026-11-03";
/// Acknowledgement text GIBS asks users to display.
pub const GIBS_ACKNOWLEDGEMENT: &str = "We acknowledge the use of imagery provided by services from NASA's Global Imagery Browse Services (GIBS), part of NASA's Earth Science Data and Information System (ESDIS).";
const GIBS_TERMS_URL: &str =
    "https://www.earthdata.nasa.gov/engage/open-data-services-software-policies/data-use-policy";
const GIBS_LICENSE: &str = "U.S. Government work, public domain (NASA Earthdata data use policy)";

pub const SOURCE_BLUE_MARBLE: &str = "gibs-bluemarble";
pub const SOURCE_VIIRS_TRUE_COLOR: &str = "gibs-viirs-truecolor";
pub const SOURCE_S2_CLOUDLESS: &str = "eox-s2cloudless-2025";
pub const SOURCE_USGS_ORTHO: &str = "usgs-tnm-orthoimagery";

/// Requests in flight per origin. GIBS asks for no bulk use; the two detail origins publish
/// no rate figure, so they get the same conservative bound (Cesium's default is 18).
const REQUESTS_IN_FLIGHT: u8 = 6;

/// The globe level from which the United States orthoimagery is drawn: the level where the
/// Sentinel-2 tier (10 m source) stops adding detail.
const USGS_FROM_GLOBE_LEVEL: u8 = 13;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageryRole {
    /// One is selected by the user and drawn at every zoom. An overlay role returns when a
    /// source that serves transparent tiles is registered (see the withdrawn coastline layer).
    Basemap,
    /// Drawn above the basemap only from [`DetailTier::from_globe_level`] down, where its finer
    /// source resolution adds real detail.
    Detail,
}

/// How the renderer fills the URL template.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TileAddressing {
    /// WMTS REST: `{TileMatrixSet}`, `{TileMatrix}`, `{TileRow}`, `{TileCol}`.
    WmtsRest,
    /// Web Mercator `{z}/{y}/{x}`, rows counted from the north.
    Xyz,
}

/// A WGS84 longitude/latitude box in degrees, west < east (no antimeridian crossing).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct GeoRectangle {
    pub west_deg: f64,
    pub south_deg: f64,
    pub east_deg: f64,
    pub north_deg: f64,
}

/// An inclusive, daily date range published by the provider.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DateCoverage {
    pub start: String,
    pub end: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TimeDimension {
    pub period: &'static str,
    /// Coverage periods as published in the capabilities on `CAPABILITIES_CHECKED_ON`.
    pub coverage: Vec<DateCoverage>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ImagerySource {
    pub id: &'static str,
    pub label: &'static str,
    pub provider: &'static str,
    pub origin: &'static str,
    pub layer: &'static str,
    /// Tile URL template; `{Time}` is substituted here, the rest by the renderer.
    pub url_template: &'static str,
    pub addressing: TileAddressing,
    /// WMTS tile matrix set; empty for [`TileAddressing::Xyz`].
    pub tile_matrix_set: &'static str,
    /// Deepest Web Mercator level the provider publishes with source pixels. Closer than
    /// that the renderer magnifies the last level; it never requests resampled tiles.
    pub maximum_level: u8,
    /// Nominal source ground sample distance in metres, as the provider states it. Pixels on
    /// screen may be coarser (tile pyramid); they are never finer than this.
    pub source_resolution_m: f64,
    /// Where the provider publishes tiles; `None` is global. Nothing is requested outside it.
    pub coverage_rectangle: Option<GeoRectangle>,
    pub max_requests_in_flight: u8,
    pub format: &'static str,
    pub role: ImageryRole,
    pub time: Option<TimeDimension>,
    /// What the pixels are, and what they are not, in plain words.
    pub capture_statement: &'static str,
    pub license: &'static str,
    pub attribution: &'static str,
    pub terms_url: &'static str,
    pub capabilities_checked_on: &'static str,
}

/// The page's imagery intent. Validated by [`resolve`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImagerySelection {
    pub source_id: String,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub coastlines: bool,
    /// Draw the detail ladder when zoomed in. Off keeps only the selected basemap (for
    /// example a single VIIRS day) at every zoom.
    #[serde(default = "detail_default")]
    pub detail: bool,
}

const fn detail_default() -> bool {
    true
}

impl Default for ImagerySelection {
    fn default() -> Self {
        Self {
            source_id: SOURCE_BLUE_MARBLE.to_owned(),
            date: None,
            coastlines: false,
            detail: detail_default(),
        }
    }
}

/// A detail source as the renderer should draw it over the selected basemap.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DetailTier {
    pub source: ImagerySource,
    pub url: String,
    /// Cesium globe surface tile level (geographic quadtree on the WGS84 ellipsoid) from
    /// which this tier is drawn; coarser surface tiles show only the layers below it. Imagery
    /// levels coarser than this are never requested for the tier, so where the provider has
    /// no tile the layer below shows instead of a coarse ancestor tile magnified on top.
    pub from_globe_level: u8,
    /// How this tier differs in time from the selected basemap, in plain words.
    pub time_relation: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ActiveImagery {
    pub source: ImagerySource,
    pub date: Option<String>,
    /// Template with `{Time}` already substituted.
    pub url: String,
    /// Set when the date lies beyond the coverage recorded at the capabilities check.
    pub coverage_note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ImageryView {
    pub active: ActiveImagery,
    pub coastlines: Option<ActiveImagery>,
    /// Whether the user asked for the detail ladder.
    pub detail_enabled: bool,
    /// Coarse to fine; empty when `detail_enabled` is false.
    pub detail: Vec<DetailTier>,
    pub sources: Vec<ImagerySource>,
    pub acknowledgement: &'static str,
    /// Today's UTC civil date, so the page can bound its date control without a clock of its own.
    pub today_utc: String,
}

fn coverage(periods: &[(&str, &str)]) -> Vec<DateCoverage> {
    periods
        .iter()
        .map(|(start, end)| DateCoverage {
            start: (*start).to_owned(),
            end: (*end).to_owned(),
        })
        .collect()
}

/// The registry. Values were read from the GIBS `epsg3857/best` capabilities on
/// [`CAPABILITIES_CHECKED_ON`]; the date coverage is the provider's statement, not OIA's.
#[must_use]
pub fn sources() -> Vec<ImagerySource> {
    vec![
        ImagerySource {
            id: SOURCE_BLUE_MARBLE,
            label: "Blue Marble (shaded relief and bathymetry)",
            provider: "NASA GIBS",
            origin: GIBS_ORIGIN,
            layer: "BlueMarble_ShadedRelief_Bathymetry",
            url_template: "https://gibs.earthdata.nasa.gov/wmts/epsg3857/best/BlueMarble_ShadedRelief_Bathymetry/default/{TileMatrixSet}/{TileMatrix}/{TileRow}/{TileCol}.jpeg",
            addressing: TileAddressing::WmtsRest,
            tile_matrix_set: "GoogleMapsCompatible_Level8",
            maximum_level: 8,
            source_resolution_m: 500.0,
            coverage_rectangle: None,
            max_requests_in_flight: REQUESTS_IN_FLIGHT,
            format: "image/jpeg",
            role: ImageryRole::Basemap,
            time: None,
            capture_statement: "Blue Marble Next Generation: a composite of MODIS observations from 2004 with shaded relief and bathymetry. It shows no particular day, no weather, and nothing from the case period. Resolution about 500 m per pixel at best.",
            license: GIBS_LICENSE,
            attribution: "NASA GIBS / Blue Marble Next Generation (Reto Stöckli, NASA Earth Observatory)",
            terms_url: GIBS_TERMS_URL,
            capabilities_checked_on: CAPABILITIES_CHECKED_ON,
        },
        ImagerySource {
            id: SOURCE_VIIRS_TRUE_COLOR,
            label: "VIIRS true colour, daily (Suomi NPP)",
            provider: "NASA GIBS",
            origin: GIBS_ORIGIN,
            layer: "VIIRS_SNPP_CorrectedReflectance_TrueColor",
            url_template: "https://gibs.earthdata.nasa.gov/wmts/epsg3857/best/VIIRS_SNPP_CorrectedReflectance_TrueColor/default/{Time}/{TileMatrixSet}/{TileMatrix}/{TileRow}/{TileCol}.jpeg",
            addressing: TileAddressing::WmtsRest,
            tile_matrix_set: "GoogleMapsCompatible_Level9",
            maximum_level: 9,
            source_resolution_m: 250.0,
            coverage_rectangle: None,
            max_requests_in_flight: REQUESTS_IN_FLIGHT,
            format: "image/jpeg",
            role: ImageryRole::Basemap,
            time: Some(TimeDimension {
                period: "P1D",
                coverage: coverage(&[
                    ("2015-11-24", "2022-07-27"),
                    ("2022-08-10", "2024-05-31"),
                    ("2024-06-04", "2024-07-11"),
                    ("2024-07-16", "2024-07-24"),
                    ("2024-07-29", "2024-11-03"),
                    ("2024-11-06", "2026-04-28"),
                    ("2026-04-30", "2026-07-10"),
                    ("2026-07-16", "2026-11-02"),
                ]),
            }),
            capture_statement: "Daily mosaic of Suomi NPP VIIRS day-side passes for the selected UTC date. Overpass time varies by longitude (about 13:30 local solar time); it is not a single instant. Cloud, haze and swath gaps are that day's data. Resolution about 250 m per pixel at best. No tiles exist before 2015-11-24, so nothing here shows the 1994 case period.",
            license: GIBS_LICENSE,
            attribution: "NASA GIBS / NASA Worldview, VIIRS SNPP Corrected Reflectance",
            terms_url: GIBS_TERMS_URL,
            capabilities_checked_on: CAPABILITIES_CHECKED_ON,
        },
        ImagerySource {
            id: SOURCE_S2_CLOUDLESS,
            label: "Sentinel-2 cloudless 2025 (EOX), 10 m",
            provider: "EOX IT Services GmbH",
            origin: EOX_ORIGIN,
            layer: "s2cloudless-2025_3857",
            url_template: "https://tiles.maps.eox.at/wmts/1.0.0/s2cloudless-2025_3857/default/GoogleMapsCompatible/{z}/{y}/{x}.jpg",
            addressing: TileAddressing::Xyz,
            tile_matrix_set: "",
            // 10 m source pixels; level 14 is 9.55 m per pixel at the equator. EOX serves
            // deeper levels, but they are upsampled from the same pixels, so none is requested.
            maximum_level: 14,
            source_resolution_m: 10.0,
            coverage_rectangle: None,
            max_requests_in_flight: REQUESTS_IN_FLIGHT,
            format: "image/jpeg",
            role: ImageryRole::Detail,
            time: None,
            capture_statement: "Cloud-free yearly mosaic built by EOX from Copernicus Sentinel-2 scenes acquired during 2025, choosing cloud-free pixels per location. Different places, and neighbouring tiles, come from different days of that year; no pixel carries its acquisition date here. Natural colour, 10 m source pixels. It shows nothing from the case period.",
            license: "CC BY-NC-SA 4.0 for non-commercial use; commercial use needs an EOX licence (https://cloudless.eox.at)",
            attribution: "EOxCloudless https://cloudless.eox.at by EOX IT Services GmbH (Contains modified Copernicus Sentinel data 2025)",
            terms_url: "https://cloudless.eox.at/documentation/license",
            capabilities_checked_on: CAPABILITIES_CHECKED_ON,
        },
        ImagerySource {
            id: SOURCE_USGS_ORTHO,
            label: "USGS The National Map orthoimagery (United States), about 1 m",
            provider: "USGS The National Map",
            origin: USGS_TNM_ORIGIN,
            layer: "USGSImageryOnly",
            url_template: "https://basemap.nationalmap.gov/arcgis/rest/services/USGSImageryOnly/MapServer/tile/{z}/{y}/{x}",
            addressing: TileAddressing::Xyz,
            tile_matrix_set: "",
            // The service is cached to 1:9,028 (level 16, 2.39 m per pixel at the equator,
            // about 1.8 m at 40° N); deeper levels answer HTTP 404.
            maximum_level: 16,
            source_resolution_m: 1.0,
            // Conterminous United States. Inside the box, places the cache has no tile for
            // (sea, Canada, Mexico) answer HTTP 404 and are counted as not published.
            coverage_rectangle: Some(GeoRectangle {
                west_deg: -125.0,
                south_deg: 24.3,
                east_deg: -66.8,
                north_deg: 49.5,
            }),
            max_requests_in_flight: REQUESTS_IN_FLIGHT,
            format: "image/jpeg",
            role: ImageryRole::Detail,
            time: None,
            capture_statement: "Orthorectified aerial photography, mostly from the USDA National Agriculture Imagery Program (NAIP), 1 m, leaf-on, flown 2017 to 2021 for the conterminous United States (service refreshed June 2024). The flight year differs by state and is not identified per tile here. Aerial, not satellite. It shows nothing from the case period.",
            license: "Public domain (USDA and USGS, U.S. Government work)",
            attribution: "USDA, USGS The National Map: Orthoimagery",
            terms_url: "https://www.usgs.gov/information-policies-and-instructions/copyrights-and-credits",
            capabilities_checked_on: CAPABILITIES_CHECKED_ON,
        },
    ]
}

/// A proleptic Gregorian civil date, used only for comparison and formatting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CivilDate {
    pub year: i64,
    pub month: u8,
    pub day: u8,
}

impl CivilDate {
    /// Strict `YYYY-MM-DD`, with the day checked against the month and leap years.
    pub fn parse(text: &str) -> Result<Self> {
        let bytes = text.as_bytes();
        let invalid = || {
            ToolError::new(
                "date_invalid",
                format!("Expected YYYY-MM-DD, got {text:?}."),
            )
        };
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return Err(invalid());
        }
        let number = |range: std::ops::Range<usize>| -> Result<i64> {
            let slice = text.get(range).ok_or_else(invalid)?;
            if !slice.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid());
            }
            slice.parse::<i64>().map_err(|_| invalid())
        };
        let year = number(0..4)?;
        let month = u8::try_from(number(5..7)?).map_err(|_| invalid())?;
        let day = u8::try_from(number(8..10)?).map_err(|_| invalid())?;
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return Err(invalid());
        }
        Ok(Self { year, month, day })
    }

    /// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
    #[must_use]
    pub fn to_days(self) -> i64 {
        let y = if self.month <= 2 {
            self.year - 1
        } else {
            self.year
        };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let m = i64::from(self.month);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// Inverse of [`to_days`](Self::to_days) (Hinnant's `civil_from_days`).
    #[must_use]
    pub fn from_days(days: i64) -> Self {
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        Self {
            year: if month <= 2 { y + 1 } else { y },
            month: u8::try_from(month).unwrap_or(1),
            day: u8::try_from(day).unwrap_or(1),
        }
    }

    #[must_use]
    pub fn today_utc() -> Self {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_secs()).unwrap_or(0))
            .unwrap_or(0);
        Self::from_days(seconds.div_euclid(86_400))
    }
}

impl std::fmt::Display for CivilDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

fn days_in_month(year: i64, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        _ => 28,
    }
}

fn find(id: &str) -> Result<ImagerySource> {
    sources()
        .into_iter()
        .find(|s| s.id == id && s.role == ImageryRole::Basemap)
        .ok_or_else(|| {
            ToolError::new(
                "imagery_unknown",
                format!("{id:?} is not a registered basemap source."),
            )
        })
}

fn activate(
    source: ImagerySource,
    date: Option<String>,
    today: CivilDate,
) -> Result<ActiveImagery> {
    let Some(time) = &source.time else {
        return Ok(ActiveImagery {
            url: source.url_template.to_owned(),
            source,
            date: None,
            coverage_note: None,
        });
    };
    let date_text = date.unwrap_or_else(|| CivilDate::from_days(today.to_days() - 1).to_string());
    let date = CivilDate::parse(&date_text)?;
    if date > today {
        return Err(ToolError::new(
            "date_future",
            format!("{date} is after today ({today} UTC); no imagery can exist for it."),
        ));
    }
    let mut covered = false;
    let mut last_end = None;
    for period in &time.coverage {
        let start = CivilDate::parse(&period.start)?;
        let end = CivilDate::parse(&period.end)?;
        if date >= start && date <= end {
            covered = true;
        }
        last_end = Some(last_end.map_or(end, |e: CivilDate| e.max(end)));
    }
    let coverage_note = if covered {
        None
    } else if last_end.is_some_and(|end| date > end) {
        Some(format!(
            "{date} is after the last day published when the capabilities were checked ({}); tiles may not exist yet. Missing tiles are a gap, not an empty scene.",
            source.capabilities_checked_on
        ))
    } else {
        return Err(ToolError::new(
            "imagery_date_uncovered",
            format!(
                "GIBS published no {} tiles for {date} when its capabilities were checked on {}. Choose a day inside a published period; a missing day is a coverage gap, not an empty scene.",
                source.layer, source.capabilities_checked_on
            ),
        ));
    };
    Ok(ActiveImagery {
        url: source.url_template.replace("{Time}", &date.to_string()),
        source,
        date: Some(date.to_string()),
        coverage_note,
    })
}

/// The detail tiers for a basemap, coarse to fine. The global Sentinel-2 tier starts one
/// globe level before the basemap runs out of source pixels (Cesium's globe level is about
/// one less than the Web Mercator imagery level it draws); the United States orthoimagery
/// starts where the Sentinel-2 tier stops adding detail.
fn detail_ladder(basemap: &ActiveImagery) -> Vec<DetailTier> {
    let basemap_time = match &basemap.date {
        Some(date) => format!(
            "Not the selected day: where this tier is drawn it replaces the {date} UTC {} pixels with imagery from another period.",
            basemap.source.layer
        ),
        None => "A different capture period from the basemap composite.".to_owned(),
    };
    let from = |source: &ImagerySource| match source.id {
        SOURCE_USGS_ORTHO => USGS_FROM_GLOBE_LEVEL,
        _ => basemap.source.maximum_level.saturating_sub(1),
    };
    sources()
        .into_iter()
        .filter(|s| s.role == ImageryRole::Detail)
        .map(|source| DetailTier {
            url: source.url_template.to_owned(),
            from_globe_level: from(&source),
            time_relation: basemap_time.clone(),
            source,
        })
        .collect()
}

/// Validates a selection against the registry and today's date.
pub fn resolve(selection: &ImagerySelection, today: CivilDate) -> Result<ImageryView> {
    let source = find(&selection.source_id)?;
    let active = activate(source, selection.date.clone(), today)?;
    if selection.coastlines {
        // Evaluated and withdrawn: GIBS `Coastlines_15m` answered HTTP 500 for tiles at levels
        // 5 to 9 and served opaque black tiles inland on 2026-11-03 (globe-instrument.md).
        return Err(ToolError::new(
            "imagery_unknown",
            "No overlay source is registered in this build.",
        ));
    }
    let coastlines = None;
    let detail = if selection.detail {
        detail_ladder(&active)
    } else {
        Vec::new()
    };
    Ok(ImageryView {
        active,
        coastlines,
        detail_enabled: selection.detail,
        detail,
        sources: sources(),
        acknowledgement: GIBS_ACKNOWLEDGEMENT,
        today_utc: today.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every registered origin; the CSP must allow exactly these.
    const REGISTERED_ORIGINS: [&str; 3] = [GIBS_ORIGIN, EOX_ORIGIN, USGS_TNM_ORIGIN];

    fn day(text: &str) -> CivilDate {
        CivilDate::parse(text).unwrap_or(CivilDate {
            year: 0,
            month: 1,
            day: 1,
        })
    }

    // GEO-02 / AGENTS.md §2: a day inside a published gap is refused as a gap, a day after
    // the recorded coverage is allowed with a note, and the future is refused.
    #[test]
    fn imagery_dates_follow_published_coverage() {
        let today = day("2026-11-03");
        let select = |date: &str| ImagerySelection {
            source_id: SOURCE_VIIRS_TRUE_COLOR.to_owned(),
            date: Some(date.to_owned()),
            coastlines: false,
            detail: false,
        };
        assert_eq!(
            resolve(&select("2022-08-01"), today).err().map(|e| e.code),
            Some("imagery_date_uncovered")
        );
        assert_eq!(
            resolve(&select("2026-11-04"), today).err().map(|e| e.code),
            Some("date_future")
        );
        assert_eq!(
            resolve(&select("2026-02-30"), today).err().map(|e| e.code),
            Some("date_invalid")
        );
        let covered = resolve(&select("2026-10-01"), today).ok();
        assert_eq!(
            covered
                .as_ref()
                .and_then(|v| v.active.coverage_note.clone()),
            None
        );
        assert!(covered.is_some_and(|v| v.active.url.contains("/2026-10-01/")));
        let beyond = resolve(&select("2026-11-03"), today).ok();
        assert!(beyond.is_some_and(|v| v.active.coverage_note.is_some()));
        // No date: yesterday, computed from the civil calendar across a month boundary.
        let default = resolve(
            &ImagerySelection {
                date: None,
                ..select("")
            },
            day("2026-03-01"),
        )
        .ok();
        assert_eq!(
            default.and_then(|v| v.active.date),
            Some("2026-02-28".to_owned())
        );
        assert_eq!(
            CivilDate::from_days(day("2024-02-29").to_days()),
            day("2024-02-29")
        );
    }

    // GEO-02 / ADR 0008 §10: the detail ladder is real registered imagery, coarse to fine,
    // labelled as another capture period than a selected day, absent when turned off, never
    // selectable as the basemap; and the CSP allows exactly the registered origins.
    #[test]
    fn detail_ladder_and_csp_follow_registry() {
        let today = day("2026-11-03");
        let viirs = ImagerySelection {
            source_id: SOURCE_VIIRS_TRUE_COLOR.to_owned(),
            date: Some("2026-10-01".to_owned()),
            coastlines: false,
            detail: true,
        };
        let view = resolve(&viirs, today).ok();
        let tiers: Vec<_> = view
            .as_ref()
            .map(|v| {
                v.detail
                    .iter()
                    .map(|t| (t.source.id, t.from_globe_level))
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(tiers, [(SOURCE_S2_CLOUDLESS, 8), (SOURCE_USGS_ORTHO, 13)]);
        assert!(view.is_some_and(|v| {
            v.detail.iter().all(|t| {
                t.time_relation.contains("2026-10-01")
                    && t.source.source_resolution_m < v.active.source.source_resolution_m
            })
        }));
        let off = resolve(
            &ImagerySelection {
                detail: false,
                ..viirs.clone()
            },
            today,
        )
        .ok();
        assert!(off.is_some_and(|v| v.detail.is_empty() && !v.detail_enabled));
        let as_basemap = ImagerySelection {
            source_id: SOURCE_USGS_ORTHO.to_owned(),
            ..viirs
        };
        assert_eq!(
            resolve(&as_basemap, today).err().map(|e| e.code),
            Some("imagery_unknown")
        );

        let conf = include_str!("../../tauri.conf.json");
        let csp = conf
            .split("\"csp\": \"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap_or_default();
        for directive in ["connect-src", "img-src"] {
            let remote: Vec<&str> = csp
                .split(';')
                .map(str::trim)
                .find(|d| d.starts_with(directive))
                .map(|d| {
                    d.split_whitespace()
                        .filter(|t| t.starts_with("https://"))
                        .collect()
                })
                .unwrap_or_default();
            assert_eq!(remote, REGISTERED_ORIGINS, "{directive}");
        }
        for source in sources() {
            assert!(REGISTERED_ORIGINS.contains(&source.origin));
            assert!(
                source
                    .url_template
                    .starts_with(&format!("{}/", source.origin))
            );
        }
    }
}
