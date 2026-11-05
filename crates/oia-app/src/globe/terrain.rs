//! Registered elevation display and camera presets. These settings never supply evidence
//! heights or perform a vertical-datum conversion (`GEO-01`, `GEO-02`, ADR 0008 §10).
use crate::local_tools::{Result, ToolError};
use serde::{Deserialize, Serialize};

pub const ORIGIN: &str = "https://elevation-tiles-prod.s3.amazonaws.com";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainSelection {
    pub enabled: bool,
    pub exaggeration: u8,
}

impl Default for TerrainSelection {
    fn default() -> Self {
        Self {
            enabled: true,
            exaggeration: 1,
        }
    }
}

impl TerrainSelection {
    pub fn validate(self) -> Result<()> {
        if !(1..=5).contains(&self.exaggeration) {
            return Err(ToolError::new(
                "terrain_invalid",
                "Terrain exaggeration must be a whole number from 1 to 5. It changes display only.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TerrainSource {
    pub id: &'static str,
    pub label: &'static str,
    pub origin: &'static str,
    pub url_template: &'static str,
    pub attribution: &'static str,
    pub contributor_attribution: &'static str,
    pub license: &'static str,
    pub terms_url: &'static str,
    pub coverage: &'static str,
    pub resolution: &'static str,
    pub height_reference: &'static str,
    pub capture_statement: &'static str,
    pub checked_on: &'static str,
    pub maximum_level: u8,
    pub tile_size: u16,
    pub max_requests_in_flight: u8,
    pub max_tile_bytes: usize,
    pub deadline_ms: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TerrainView {
    pub selection: TerrainSelection,
    pub source: TerrainSource,
    pub statement: &'static str,
}

#[must_use]
pub fn view(selection: TerrainSelection) -> TerrainView {
    TerrainView {
        selection,
        source: TerrainSource {
            id: "mapzen-terrarium",
            label: "Mapzen terrain tiles / AWS Open Data",
            origin: ORIGIN,
            url_template: "https://elevation-tiles-prod.s3.amazonaws.com/terrarium/{z}/{x}/{y}.png",
            attribution: "Mapzen terrain tiles · USGS, NOAA and other contributors · AWS Open Data",
            contributor_attribution: "ArcticDEM DEM(s) created from DigitalGlobe imagery, funded under NSF awards 1043681, 1559691, 1542736; © Commonwealth of Australia (Geoscience Australia) 2017; © offene Daten Österreichs – Digitales Geländemodell (DGM) Österreich; Canada: contains information licensed under the Open Government Licence – Canada; Europe: produced using Copernicus data and information funded by the European Union – EU-DEM layers; ETOPO1: NOAA; Mexico: INEGI, Continental relief, 2016; New Zealand: Copyright 2011 Crown copyright (c) Land Information New Zealand and the New Zealand Government, all rights reserved; Norway: © Kartverket; United Kingdom: © Environment Agency copyright and/or database right 2015, all rights reserved; 3DEP, GMTED2010, SRTM: courtesy USGS.",
            license: "Multiple open-data licences: public domain, CC BY and national open-government terms; see contributor attribution and source terms.",
            terms_url: "https://github.com/tilezen/joerd/blob/master/docs/attribution.md",
            coverage: "Web Mercator, approximately 85.05° S to 85.05° N; source coverage/resolution varies. Polar caps are outside this terrain source.",
            resolution: "Source-dependent (including SRTM, 3DEP and coarse ETOPO1); level 14 display cap, then upsampled. Encoded fractions are not accuracy.",
            height_reference: "Source-dependent elevation/bathymetry in metres, not established as WGS84 ellipsoid heights. Used as render offsets only; no geoid conversion, AGL, calibrated line of sight or measurement claim.",
            capture_statement: "Multi-source derived elevation mosaic; acquisition dates vary and are not supplied per displayed tile. Not the case period or selected imagery day; object modification time is not acquisition time.",
            checked_on: "2026-11-05",
            maximum_level: 14,
            tile_size: 256,
            max_requests_in_flight: 4,
            max_tile_bytes: 524_288,
            deadline_ms: 10_000,
        },
        statement: "Terrain is display context. Draping locates symbols on the rendered surface only; missing evidence heights remain unknown. Exaggeration changes this display, never source values or analysis. Failed tiles leave a visible gap status; no synthetic zero-height replacement is supplied.",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Perspective {
    pub id: &'static str,
    pub label: &'static str,
    pub pitch_deg: f64,
    pub key: &'static str,
}

pub const PERSPECTIVES: [Perspective; 3] = [
    Perspective {
        id: "top_down",
        label: "Top-down",
        pitch_deg: -90.0,
        key: "1",
    },
    Perspective {
        id: "oblique",
        label: "Oblique",
        pitch_deg: -45.0,
        key: "2",
    },
    Perspective {
        id: "horizon",
        label: "Horizon",
        pitch_deg: -15.0,
        key: "3",
    },
];
