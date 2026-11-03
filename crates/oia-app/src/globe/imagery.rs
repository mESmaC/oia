//! Registered imagery sources for the Globe instrument (`GEO-02`, ADR 0008 §4 amendment).
//!
//! Every origin the page may load tiles from is listed here with its licence,
//! attribution, capture statement and the date the WMTS capabilities were checked.
//! The page never composes a tile URL from anything that is not in this registry.
//! Dates are validated here so a day outside the published coverage is refused as a
//! documented gap rather than drawn as an empty scene (`AGENTS.md` §2).

use crate::local_tools::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// The only remote origin registered for globe imagery in this slice.
pub const GIBS_ORIGIN: &str = "https://gibs.earthdata.nasa.gov";
/// When the GIBS `epsg3857/best` WMTS capabilities were last read for this registry.
pub const CAPABILITIES_CHECKED_ON: &str = "2026-11-03";
/// Acknowledgement text GIBS asks users to display.
pub const GIBS_ACKNOWLEDGEMENT: &str = "We acknowledge the use of imagery provided by services from NASA's Global Imagery Browse Services (GIBS), part of NASA's Earth Science Data and Information System (ESDIS).";
const GIBS_TERMS_URL: &str =
    "https://www.earthdata.nasa.gov/engage/open-data-services-software-policies/data-use-policy";
const GIBS_LICENSE: &str = "U.S. Government work, public domain (NASA Earthdata data use policy)";

pub const SOURCE_BLUE_MARBLE: &str = "gibs-bluemarble";
pub const SOURCE_VIIRS_TRUE_COLOR: &str = "gibs-viirs-truecolor";
pub const SOURCE_COASTLINES: &str = "gibs-coastlines";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageryRole {
    Basemap,
    Overlay,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ImagerySource {
    pub id: &'static str,
    pub label: &'static str,
    pub provider: &'static str,
    pub origin: &'static str,
    pub layer: &'static str,
    /// WMTS REST template; `{Time}` is substituted here, the rest by the renderer.
    pub url_template: &'static str,
    pub tile_matrix_set: &'static str,
    pub maximum_level: u8,
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
}

impl Default for ImagerySelection {
    fn default() -> Self {
        Self {
            source_id: SOURCE_BLUE_MARBLE.to_owned(),
            date: None,
            coastlines: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ActiveImagery {
    pub source: ImagerySource,
    pub date: Option<String>,
    /// Template with `{Time}` already substituted.
    pub url: String,
    /// Set when the date lies beyond the coverage recorded at the capabilities check.
    pub coverage_note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ImageryView {
    pub active: ActiveImagery,
    pub coastlines: Option<ActiveImagery>,
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
            tile_matrix_set: "GoogleMapsCompatible_Level8",
            maximum_level: 8,
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
            tile_matrix_set: "GoogleMapsCompatible_Level9",
            maximum_level: 9,
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
            capture_statement: "Daily mosaic of Suomi NPP VIIRS day-side passes for the selected UTC date. Overpass time varies by longitude (about 13:30 local solar time); it is not a single instant. Cloud, haze and swath gaps are that day's data. Resolution about 250 m per pixel at best. Tiles older than the 1994 case period do not exist; nothing here shows the case.",
            license: GIBS_LICENSE,
            attribution: "NASA GIBS / NASA Worldview, VIIRS SNPP Corrected Reflectance",
            terms_url: GIBS_TERMS_URL,
            capabilities_checked_on: CAPABILITIES_CHECKED_ON,
        },
        ImagerySource {
            id: SOURCE_COASTLINES,
            label: "Coastlines (reference lines)",
            provider: "NASA GIBS",
            origin: GIBS_ORIGIN,
            layer: "Coastlines_15m",
            url_template: "https://gibs.earthdata.nasa.gov/wmts/epsg3857/best/Coastlines_15m/default/{TileMatrixSet}/{TileMatrix}/{TileRow}/{TileCol}.png",
            tile_matrix_set: "GoogleMapsCompatible_Level13",
            maximum_level: 13,
            format: "image/png",
            role: ImageryRole::Overlay,
            time: None,
            capture_statement: "Cartographic coastline outlines rendered by GIBS from the Natural Earth and OpenStreetMap-derived reference set. Lines, not observations.",
            license: GIBS_LICENSE,
            attribution: "NASA GIBS reference layer (Coastlines_15m)",
            terms_url: GIBS_TERMS_URL,
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
    sources().into_iter().find(|s| s.id == id).ok_or_else(|| {
        ToolError::new(
            "imagery_unknown",
            format!("{id:?} is not a registered imagery source."),
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

/// Validates a selection against the registry and today's date.
pub fn resolve(selection: &ImagerySelection, today: CivilDate) -> Result<ImageryView> {
    let source = find(&selection.source_id)?;
    if source.role != ImageryRole::Basemap {
        return Err(ToolError::new(
            "imagery_not_basemap",
            format!("{} is an overlay, not a basemap.", source.label),
        ));
    }
    let active = activate(source, selection.date.clone(), today)?;
    let coastlines = if selection.coastlines {
        Some(activate(find(SOURCE_COASTLINES)?, None, today)?)
    } else {
        None
    };
    Ok(ImageryView {
        active,
        coastlines,
        sources: sources(),
        acknowledgement: GIBS_ACKNOWLEDGEMENT,
        today_utc: today.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
