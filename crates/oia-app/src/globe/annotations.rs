//! Analyst map annotations on the Globe (`#165`, refs `#23`; `CASE-03`, `GEO-01`).
//!
//! An annotation is something the analyst drew: a pin, a line, an area or a range ring, with
//! a label, an optional note and an optional reference to a case record. It is an
//! [`ObjectType::Annotation`], never an observation, and it never changes the record it refers
//! to (`CASE-03`). Rust owns the annotations, validates every vertex and computes the measures
//! the page prints; the page only collects clicks and draws what [`AnnotationLayerView`] says.
//!
//! The manual draw flow follows God's Eye View's whiteboard draw tool (`src/annotations/
//! drawMode.js`, `drawTool.js` at `0d1a681`, MIT; see `docs/prior-art.md`). Deliberate
//! differences: vertices are validated and refused rather than silently dropped, measures are
//! computed here and labelled with their method, there is no colour choice (amber, red and
//! violet carry status meaning in visual language v2 §5.1), and nothing fades out on a timer.
//!
//! Annotations live for this session only. Durable storage waits on `#11`/`#14`; the view says
//! so rather than implying the marks are saved.

use super::{MAX_RING_POINTS, MAX_TEXT_BYTES, Wgs84Point};
use crate::local_tools::{Result, ToolError};
use oia_domain::evidence::ObjectType;
use oia_time_geo::{EARTH_RADIUS_METERS, GeoPointWgs84};
use serde::{Deserialize, Serialize};

/// Layer id of the annotation layer in the Globe's layer list.
pub const LAYER_ANNOTATIONS: &str = "annotations";
/// Live annotations per session; protects the renderer from unbounded entities and tags.
pub const MAX_ANNOTATIONS: usize = 120;
/// Vertices in one line.
pub const MAX_LINE_VERTICES: usize = 512;
/// Vertices in one area ring, the same bound as a case region.
pub const MAX_AREA_VERTICES: usize = MAX_RING_POINTS;
pub const MAX_LABEL_BYTES: usize = 256;
pub const MAX_NOTE_BYTES: usize = MAX_TEXT_BYTES;
/// Consecutive vertices closer than this are one click (the second half of a double-click),
/// and a ring radius below it is a mis-click. The page avoids sending them; Rust refuses them.
pub const MIN_SEPARATION_M: f64 = 0.5;
/// A segment this long has nearly antipodal ends, so its great-circle path is ambiguous.
pub const MAX_SEGMENT_M: f64 = 19_000_000.0;
/// Longest range-ring radius: a quarter of a great circle.
pub const MAX_RING_RADIUS_M: f64 = 10_000_000.0;
/// Prefix of every annotation id; ids are never reused within a session.
pub const ID_PREFIX: &str = "ann-";
/// Bearing step of a range ring's drawn outline.
pub const RING_OUTLINE_STEP_DEG: u32 = 5;

const MEASURE_METHOD: &str = "Great-circle distance on a sphere of radius 6,371,008.8 m (IUGG mean radius, oia-time-geo). Not an ellipsoidal geodesic or a survey; positions are where the analyst clicked on the WGS84 ellipsoid with no terrain.";
const STATEMENT: &str = "Drawn by the analyst in this session. Annotations are analyst notes on the map, not observations, and never change the case records they refer to. Kept in memory only: they are lost when OIA closes.";

fn invalid(message: impl ToString) -> ToolError {
    ToolError::new("annotation_invalid", message)
}

/// Geometry as drawn. Vertices are kept exactly as entered; nothing is snapped or simplified.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case", deny_unknown_fields)]
pub enum AnnotationGeometry {
    Pin {
        point: Wgs84Point,
    },
    /// An open path through the vertices in order.
    Line {
        vertices: Vec<Wgs84Point>,
    },
    /// A closed ring; the first vertex is not repeated at the end.
    Area {
        ring: Vec<Wgs84Point>,
    },
    /// A circle of the great-circle distance from `centre` to `edge`, both as clicked.
    Ring {
        centre: Wgs84Point,
        edge: Wgs84Point,
    },
}

fn point(name: &str, p: &Wgs84Point) -> Result<GeoPointWgs84> {
    GeoPointWgs84::new(p.lat_deg, p.lon_deg).map_err(|e| {
        ToolError::new(
            "value_out_of_range",
            format!("{name}: {e} (got {}, {}).", p.lat_deg, p.lon_deg),
        )
    })
}

/// Validates consecutive segments and returns their lengths in order.
fn segments(name: &str, points: &[GeoPointWgs84], closed: bool) -> Result<Vec<f64>> {
    let mut lengths = Vec::with_capacity(points.len());
    let pairs = points.windows(2).map(|w| (w[0], w[1]));
    let closing = if closed {
        points.first().copied().zip(points.last().copied())
    } else {
        None
    };
    for (i, (a, b)) in pairs
        .chain(closing.map(|(first, last)| (last, first)))
        .enumerate()
    {
        let d = a.great_circle_distance_m(b);
        if !d.is_finite() || d < MIN_SEPARATION_M {
            let which = if closed && i + 1 == points.len() {
                "The last vertex repeats the first; an area closes itself, so leave it out."
                    .to_owned()
            } else {
                format!(
                    "Vertices {i} and {} are {d:.2} m apart; consecutive vertices must be at least {MIN_SEPARATION_M} m apart.",
                    i + 1
                )
            };
            return Err(invalid(format!("{name}: {which}")));
        }
        if d > MAX_SEGMENT_M {
            return Err(invalid(format!(
                "{name}: the segment after vertex {i} is {:.0} km, so its ends are nearly antipodal and the path is ambiguous. Add a vertex part way.",
                d / 1000.0
            )));
        }
        lengths.push(d);
    }
    Ok(lengths)
}

/// Measures derived from the geometry, with the method that produced them.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AnnotationMeasures {
    pub vertices: usize,
    /// Line: sum of its segments.
    pub length_m: Option<f64>,
    /// Area: the closed ring's boundary.
    pub perimeter_m: Option<f64>,
    /// Range ring: centre to edge.
    pub radius_m: Option<f64>,
    pub method: &'static str,
}

impl AnnotationGeometry {
    /// Validates the geometry and returns its measures. Nothing is clamped or repaired.
    pub fn measure(&self) -> Result<AnnotationMeasures> {
        let mut m = AnnotationMeasures {
            vertices: 0,
            length_m: None,
            perimeter_m: None,
            radius_m: None,
            method: MEASURE_METHOD,
        };
        match self {
            Self::Pin { point: p } => {
                point("pin", p)?;
                m.vertices = 1;
            }
            Self::Line { vertices } => {
                if vertices.len() < 2 || vertices.len() > MAX_LINE_VERTICES {
                    return Err(invalid(format!(
                        "A line has {} vertices; between 2 and {MAX_LINE_VERTICES} are accepted.",
                        vertices.len()
                    )));
                }
                let points = vertices
                    .iter()
                    .enumerate()
                    .map(|(i, v)| point(&format!("line vertex {i}"), v))
                    .collect::<Result<Vec<_>>>()?;
                m.vertices = points.len();
                m.length_m = Some(segments("line", &points, false)?.iter().sum());
            }
            Self::Area { ring } => {
                if ring.len() < 3 || ring.len() > MAX_AREA_VERTICES {
                    return Err(invalid(format!(
                        "An area has {} vertices; between 3 and {MAX_AREA_VERTICES} are accepted.",
                        ring.len()
                    )));
                }
                let points = ring
                    .iter()
                    .enumerate()
                    .map(|(i, v)| point(&format!("area vertex {i}"), v))
                    .collect::<Result<Vec<_>>>()?;
                m.vertices = points.len();
                m.perimeter_m = Some(segments("area", &points, true)?.iter().sum());
            }
            Self::Ring { centre, edge } => {
                let c = point("ring centre", centre)?;
                let e = point("ring edge", edge)?;
                let r = c.great_circle_distance_m(e);
                if !r.is_finite() || !(MIN_SEPARATION_M..=MAX_RING_RADIUS_M).contains(&r) {
                    return Err(invalid(format!(
                        "A range ring radius of {r:.2} m is outside {MIN_SEPARATION_M} m to {:.0} km.",
                        MAX_RING_RADIUS_M / 1000.0
                    )));
                }
                m.vertices = 2;
                m.radius_m = Some(r);
            }
        }
        Ok(m)
    }

    /// The drawn outline of a range ring: the spherical destination point at the ring radius
    /// for every [`RING_OUTLINE_STEP_DEG`] of bearing from north, on the same sphere as the
    /// measure. Display geometry only; `None` for other shapes and for a ring that does not
    /// validate.
    fn ring_outline(&self) -> Option<Vec<Wgs84Point>> {
        let Self::Ring { centre, edge } = self else {
            return None;
        };
        let c = point("ring centre", centre).ok()?;
        let e = point("ring edge", edge).ok()?;
        let delta = c.great_circle_distance_m(e) / EARTH_RADIUS_METERS;
        let (lat1, lon1) = (
            c.latitude_deg().to_radians(),
            c.longitude_deg().to_radians(),
        );
        let outline = (0..360 / RING_OUTLINE_STEP_DEG)
            .map(|i| {
                let theta = f64::from(i * RING_OUTLINE_STEP_DEG).to_radians();
                let lat2 = (lat1.sin() * delta.cos() + lat1.cos() * delta.sin() * theta.cos())
                    .clamp(-1.0, 1.0)
                    .asin();
                let lon2 = lon1
                    + (theta.sin() * delta.sin() * lat1.cos())
                        .atan2(delta.cos() - lat1.sin() * lat2.sin());
                // Wrap into [-180, 180) so the point validates; the meridian is unchanged.
                let lon_deg = (lon2.to_degrees() + 540.0).rem_euclid(360.0) - 180.0;
                Wgs84Point {
                    lat_deg: lat2.to_degrees(),
                    lon_deg,
                }
            })
            .collect();
        Some(outline)
    }

    fn shape(&self) -> &'static str {
        match self {
            Self::Pin { .. } => "pin",
            Self::Line { .. } => "line",
            Self::Area { .. } => "area",
            Self::Ring { .. } => "ring",
        }
    }
}

/// What the page sends to create an annotation.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationDraft {
    pub label: String,
    #[serde(default)]
    pub note: String,
    /// A case record this annotation is about. The record is not changed.
    #[serde(default)]
    pub about_record: Option<String>,
    pub geometry: AnnotationGeometry,
}

/// What the page sends to revise an annotation's text or record reference. Geometry is not
/// edited in place; redraw it as a new annotation so the drawn vertices stay as entered.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationEdit {
    pub label: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub about_record: Option<String>,
}

fn check_text(label: &str, note: &str) -> Result<()> {
    if label.trim().is_empty() {
        return Err(invalid("An annotation needs a label."));
    }
    if label.len() > MAX_LABEL_BYTES {
        return Err(invalid(format!(
            "The label is {} bytes; the limit is {MAX_LABEL_BYTES}.",
            label.len()
        )));
    }
    if label.chars().any(char::is_control) {
        return Err(invalid(
            "The label is one line; put line breaks and tabs in the note.",
        ));
    }
    if note.len() > MAX_NOTE_BYTES {
        return Err(invalid(format!(
            "The note is {} bytes; the limit is {MAX_NOTE_BYTES}.",
            note.len()
        )));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
struct Annotation {
    id: String,
    label: String,
    note: String,
    about_record: Option<String>,
    geometry: AnnotationGeometry,
    measures: AnnotationMeasures,
    created_at_ms: i64,
    revised_at_ms: Option<i64>,
    revision: u32,
}

/// One annotation as the page draws and lists it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AnnotationView {
    pub id: String,
    /// Display order within the session, `A1`, `A2`, …; not reused after a delete.
    pub tag: String,
    pub label: String,
    pub note: String,
    pub shape: &'static str,
    pub geometry: AnnotationGeometry,
    pub measures: AnnotationMeasures,
    /// Range ring outline to draw (see `AnnotationGeometry::ring_outline`); `None` otherwise.
    pub outline: Option<Vec<Wgs84Point>>,
    pub about_record: Option<String>,
    /// Label of the referenced record, or `None` when no record is referenced.
    pub about_label: Option<String>,
    /// OIA creation time, Unix milliseconds UTC. Not an event, capture or source time.
    pub created_at_ms: i64,
    pub revised_at_ms: Option<i64>,
    /// 0 for the version as drawn; incremented by each text edit.
    pub revision: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AnnotationLimits {
    pub max_annotations: usize,
    pub max_line_vertices: usize,
    pub max_area_vertices: usize,
    pub max_label_bytes: usize,
    pub max_note_bytes: usize,
    pub min_separation_m: f64,
    pub max_ring_radius_m: f64,
}

/// The annotation layer as the page sees it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AnnotationLayerView {
    pub layer_id: &'static str,
    /// The shared evidence vocabulary code (`oia_domain::evidence::ObjectType`).
    pub object_type: &'static str,
    pub visible: bool,
    pub retention: &'static str,
    pub statement: &'static str,
    pub earth_radius_m: f64,
    pub items: Vec<AnnotationView>,
    pub limits: AnnotationLimits,
}

/// Rust-owned annotation state for the session.
#[derive(Clone, Debug)]
pub struct AnnotationState {
    items: Vec<Annotation>,
    next: u64,
    visible: bool,
}

impl Default for AnnotationState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            next: 1,
            visible: true,
        }
    }
}

impl AnnotationState {
    pub fn is_annotation_id(id: &str) -> bool {
        id.starts_with(ID_PREFIX)
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn count(&self) -> usize {
        self.items.len()
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Whether `id` names an annotation that is drawn now.
    pub fn selectable(&self, id: &str) -> bool {
        self.visible && self.items.iter().any(|a| a.id == id)
    }

    fn check_record(about: Option<&str>, known_record: &dyn Fn(&str) -> bool) -> Result<()> {
        match about {
            Some(id) if !known_record(id) => Err(ToolError::new(
                "unknown_record",
                format!("{id:?} is not a record of the open case."),
            )),
            _ => Ok(()),
        }
    }

    /// Validates and adds a drawn annotation; returns its id. A refused draft changes nothing.
    pub fn create(
        &mut self,
        draft: AnnotationDraft,
        now_ms: i64,
        known_record: &dyn Fn(&str) -> bool,
    ) -> Result<String> {
        if self.items.len() >= MAX_ANNOTATIONS {
            return Err(ToolError::new(
                "annotation_limit",
                format!(
                    "This session already has {MAX_ANNOTATIONS} annotations, the limit. Delete some first."
                ),
            ));
        }
        check_text(&draft.label, &draft.note)?;
        Self::check_record(draft.about_record.as_deref(), known_record)?;
        let measures = draft.geometry.measure()?;
        let id = format!("{ID_PREFIX}{}", self.next);
        self.next += 1;
        self.items.push(Annotation {
            id: id.clone(),
            label: draft.label,
            note: draft.note,
            about_record: draft.about_record,
            geometry: draft.geometry,
            measures,
            created_at_ms: now_ms,
            revised_at_ms: None,
            revision: 0,
        });
        // The page is told nothing is drawn while the layer is hidden; show it so the new mark
        // is visible where it was just drawn.
        self.visible = true;
        Ok(id)
    }

    pub fn update(
        &mut self,
        id: &str,
        edit: AnnotationEdit,
        now_ms: i64,
        known_record: &dyn Fn(&str) -> bool,
    ) -> Result<()> {
        check_text(&edit.label, &edit.note)?;
        Self::check_record(edit.about_record.as_deref(), known_record)?;
        let item = self
            .items
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| unknown(id))?;
        if item.label == edit.label
            && item.note == edit.note
            && item.about_record == edit.about_record
        {
            return Ok(());
        }
        item.label = edit.label;
        item.note = edit.note;
        item.about_record = edit.about_record;
        item.revised_at_ms = Some(now_ms);
        item.revision = item.revision.saturating_add(1);
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> Result<()> {
        let before = self.items.len();
        self.items.retain(|a| a.id != id);
        if self.items.len() == before {
            return Err(unknown(id));
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn view(&self, record_label: &dyn Fn(&str) -> Option<String>) -> AnnotationLayerView {
        AnnotationLayerView {
            layer_id: LAYER_ANNOTATIONS,
            object_type: ObjectType::Annotation.as_str(),
            visible: self.visible,
            retention: "session only: kept in memory until OIA closes; not saved to a case",
            statement: STATEMENT,
            earth_radius_m: EARTH_RADIUS_METERS,
            items: self
                .items
                .iter()
                .map(|a| AnnotationView {
                    id: a.id.clone(),
                    tag: format!("A{}", a.id.trim_start_matches(ID_PREFIX)),
                    label: a.label.clone(),
                    note: a.note.clone(),
                    shape: a.geometry.shape(),
                    geometry: a.geometry.clone(),
                    measures: a.measures.clone(),
                    outline: a.geometry.ring_outline(),
                    about_record: a.about_record.clone(),
                    about_label: a.about_record.as_deref().and_then(record_label),
                    created_at_ms: a.created_at_ms,
                    revised_at_ms: a.revised_at_ms,
                    revision: a.revision,
                })
                .collect(),
            limits: AnnotationLimits {
                max_annotations: MAX_ANNOTATIONS,
                max_line_vertices: MAX_LINE_VERTICES,
                max_area_vertices: MAX_AREA_VERTICES,
                max_label_bytes: MAX_LABEL_BYTES,
                max_note_bytes: MAX_NOTE_BYTES,
                min_separation_m: MIN_SEPARATION_M,
                max_ring_radius_m: MAX_RING_RADIUS_M,
            },
        }
    }
}

fn unknown(id: &str) -> ToolError {
    let shown: String = id.chars().take(64).collect();
    ToolError::new(
        "unknown_annotation",
        format!("{shown:?} is not an annotation of this session."),
    )
}
