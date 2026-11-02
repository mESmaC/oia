//! Synthetic case fixture: the "1994 valley accounts" case from the merged
//! shell mockup. Nothing here is an observation, a real document, or a real
//! place. Every record carries `fixture: true` in its envelope, unknown values
//! are explicit, and positions are kilometre offsets from the reported region
//! centre rather than coordinates.

use serde::{Deserialize, Serialize};

use crate::shell::{AreaSummary, ClockMode, LocalInterval, SharedContext};

pub const CASE_ID: &str = "FIXTURE-CASE-0001";
pub const DOC_ID: &str = "FIXTURE-DOC-0007";
pub const DOC_VERSION: u32 = 2;

/// Reported region size, used only to scale the canvas.
pub const REGION_RADIUS_KM: f64 = 6.0;

/// Longest query accepted by the fixture search.
pub const MAX_QUERY_CHARS: usize = 200;

#[must_use]
pub fn initial_context() -> SharedContext {
    SharedContext {
        case_id: CASE_ID.to_owned(),
        case_title: "1994 valley accounts".to_owned(),
        area: Some(AreaSummary {
            label: "Reported valley region".to_owned(),
            approximate: true,
            extent_km: Some(REGION_RADIUS_KM * 2.0),
        }),
        interval: LocalInterval {
            date: "1994-03-14".to_owned(),
            start_min: 20 * 60,
            end_min: 23 * 60,
            zone: "America/Denver".to_owned(),
            reference: "reported local".to_owned(),
        },
        clock_min: 21 * 60 + 40,
        clock_mode: ClockMode::Historical,
        selection: None,
        timeline_focus: None,
        shared_filter: None,
    }
}

/// Glyph plus label plus tone: colour is the fourth cue (design §6).
#[derive(Debug, Clone, Serialize)]
pub struct Badge {
    pub glyph: &'static str,
    pub label: String,
    /// neutral | ok | warn | bad | gen | sel
    pub tone: &'static str,
}

fn badge(glyph: &'static str, label: &str, tone: &'static str) -> Badge {
    Badge {
        glyph,
        label: label.to_owned(),
        tone,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InventoryRow {
    pub origin: Badge,
    pub count: u32,
    pub treatment: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct HypothesisRow {
    pub id: String,
    pub text: String,
    pub supporting: u32,
    pub contradicting: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivityRow {
    pub when: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaseHome {
    pub id: String,
    pub version: u32,
    pub title: String,
    pub question: String,
    pub visibility: &'static str,
    pub area: AreaSummary,
    pub interval: LocalInterval,
    pub topics: Vec<&'static str>,
    pub inventory: Vec<InventoryRow>,
    pub hypotheses: Vec<HypothesisRow>,
    pub open_questions: Vec<String>,
    pub needs_reassessment: Vec<String>,
    pub recent_activity: Vec<ActivityRow>,
}

#[must_use]
pub fn case_home() -> CaseHome {
    let ctx = initial_context();
    CaseHome {
        id: CASE_ID.to_owned(),
        version: 3,
        title: ctx.case_title,
        question: "Why do the three accounts disagree on direction?".to_owned(),
        visibility: "private",
        area: ctx.area.unwrap_or(AreaSummary {
            label: "unknown".to_owned(),
            approximate: true,
            extent_km: None,
        }),
        interval: ctx.interval,
        topics: vec!["uap", "witness accounts", "1994"],
        inventory: vec![
            InventoryRow {
                origin: badge("◉", "Account", "neutral"),
                count: 3,
                treatment: "solid",
            },
            InventoryRow {
                origin: badge("▣", "Source", "neutral"),
                count: 1,
                treatment: "solid",
            },
            InventoryRow {
                origin: badge("❝", "Claim", "neutral"),
                count: 2,
                treatment: "solid",
            },
            InventoryRow {
                origin: badge("◇", "Inference", "warn"),
                count: 1,
                treatment: "dotted",
            },
        ],
        hypotheses: vec![HypothesisRow {
            id: "FIXTURE-HYP-0001".to_owned(),
            text: "One object moving north-west to south-east, seen from three places".to_owned(),
            supporting: 1,
            contradicting: 1,
        }],
        open_questions: vec![
            "Where was account B's observer standing?".to_owned(),
            "Does 'east of the ridge' in account C refer to the same ridge as account A?"
                .to_owned(),
        ],
        needs_reassessment: Vec::new(),
        recent_activity: vec![
            ActivityRow {
                when: "fixture · step 3".to_owned(),
                text: "Claim FIXTURE-CLM-0002 recorded from FIXTURE-DOC-0007 v2 p5.¶1".to_owned(),
            },
            ActivityRow {
                when: "fixture · step 2".to_owned(),
                text: "FIXTURE-DOC-0007 v2 acquired; OCR queued".to_owned(),
            },
            ActivityRow {
                when: "fixture · step 1".to_owned(),
                text: "Case created (private)".to_owned(),
            },
        ],
    }
}

/// Kilometre offsets from the reported region centre; east and north positive.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Km {
    pub east: f64,
    pub north: f64,
}

const fn km(east: f64, north: f64) -> Km {
    Km { east, north }
}

#[derive(Debug, Clone, Serialize)]
pub struct ObserverMark {
    pub account_id: String,
    pub label: &'static str,
    /// `None` is drawn as an open dashed marker with a question mark.
    pub position: Option<Km>,
    pub position_note: &'static str,
    pub time_label: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrackMark {
    pub account_id: String,
    pub from: Km,
    pub to: Km,
    pub label: &'static str,
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegionMark {
    pub account_id: String,
    pub polygon: Vec<Km>,
    pub label: &'static str,
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Wedge {
    pub account_id: String,
    pub apex: Km,
    pub left: Km,
    pub right: Km,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaseGeometry {
    pub basemap: &'static str,
    pub scale_note: &'static str,
    pub region_radius_km: f64,
    pub ring_km: Vec<f64>,
    pub ridge: Vec<Km>,
    pub ridge_label: &'static str,
    pub observers: Vec<ObserverMark>,
    pub tracks: Vec<TrackMark>,
    pub regions: Vec<RegionMark>,
    pub wedges: Vec<Wedge>,
}

#[must_use]
pub fn case_geometry() -> CaseGeometry {
    CaseGeometry {
        basemap: "none",
        scale_note: "Scale from reported region size (approx.)",
        region_radius_km: REGION_RADIUS_KM,
        ring_km: vec![3.0, 6.0, 9.0],
        ridge: vec![
            km(-5.2, -6.0),
            km(-3.3, -3.6),
            km(-1.9, -2.6),
            km(-1.0, -0.5),
            km(0.5, 3.6),
            km(2.1, 6.0),
        ],
        ridge_label: "Ridge line · sketch from accounts",
        observers: vec![
            ObserverMark {
                account_id: "FIXTURE-ACC-A".to_owned(),
                label: "A",
                position: Some(km(4.5, -2.3)),
                position_note: "Eastern road · 21:40 ±15",
                time_label: Some("21:40 ±15 min"),
            },
            ObserverMark {
                account_id: "FIXTURE-ACC-B".to_owned(),
                label: "B",
                position: None,
                position_note: "Position not recorded",
                time_label: None,
            },
        ],
        tracks: vec![TrackMark {
            account_id: "FIXTURE-ACC-A".to_owned(),
            from: km(-4.3, 4.8),
            to: km(2.9, -1.2),
            label: "A · NW → SE · ≈ 8 min",
            note: "Reported track · not measured",
        }],
        regions: vec![RegionMark {
            account_id: "FIXTURE-ACC-C".to_owned(),
            polygon: vec![km(0.5, 3.1), km(6.4, 3.6), km(7.1, -2.6), km(-0.5, -2.6)],
            label: "C · \"east of the ridge\"",
            note: "2 lights · no bearing · no time",
        }],
        wedges: vec![Wedge {
            account_id: "FIXTURE-ACC-A".to_owned(),
            apex: km(4.5, -2.3),
            left: km(0.5, 2.4),
            right: km(1.2, 3.1),
        }],
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    Approximate,
    Recorded,
    NotRecorded,
    RegionOnly,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimeCoverage {
    pub event: Coverage,
    pub received: Coverage,
    pub position: Coverage,
}

#[derive(Debug, Clone, Serialize)]
pub struct Bearing {
    pub degrees: f64,
    pub plus_minus_degrees: f64,
    pub text: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct KvRow {
    pub key: &'static str,
    pub value: String,
    /// true when the value is an identifier, hash, time, or count (monospace).
    pub mono: bool,
}

fn kv(key: &'static str, value: &str, mono: bool) -> KvRow {
    KvRow {
        key,
        value: value.to_owned(),
        mono,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ObjectDetail {
    Account {
        id: String,
        version: u32,
        label: String,
        origin: Badge,
        anchor: String,
        assessment: Badge,
        review: Badge,
        conflicts: Vec<Badge>,
        source_record: Vec<KvRow>,
        bearing: Option<Bearing>,
        times: TimeCoverage,
        timeline_focus: Option<(u32, u32)>,
        your_notes: Vec<String>,
    },
    Source {
        id: String,
        version: u32,
        title: String,
        origin: Badge,
        availability: Badge,
        health: Badge,
        source_record: Vec<KvRow>,
        pages: u32,
        unreadable_pages: Vec<u32>,
        lineage: Vec<String>,
        your_notes: Vec<String>,
    },
    Hypothesis {
        id: String,
        version: u32,
        text: String,
        origin: Badge,
        supporting: Vec<String>,
        contradicting: Vec<String>,
        discriminating: String,
    },
    Claim {
        id: String,
        version: u32,
        proposition: String,
        claimant: String,
        anchor: String,
        assessment: Badge,
        review: Badge,
        supporting: Vec<String>,
        contradicting: Vec<String>,
    },
}

/// Timeline focus for an object, in minutes after local midnight.
#[must_use]
pub fn object_focus(object_id: &str) -> Option<(u32, u32)> {
    match object_id {
        "FIXTURE-ACC-A" | "FIXTURE-CLM-0001" => Some((21 * 60 + 25, 21 * 60 + 55)),
        _ => None,
    }
}

/// Current version of a fixture object, if it exists.
#[must_use]
pub fn object_version(object_id: &str) -> Option<u32> {
    match object_id {
        "FIXTURE-ACC-A" | "FIXTURE-ACC-B" | "FIXTURE-ACC-C" => Some(1),
        DOC_ID => Some(DOC_VERSION),
        "FIXTURE-HYP-0001" => Some(2),
        "FIXTURE-CLM-0001" | "FIXTURE-CLM-0002" => Some(1),
        _ => None,
    }
}

fn account(
    id: &str,
    label: &str,
    anchor: &str,
    conflicts: Vec<Badge>,
    source_record: Vec<KvRow>,
    bearing: Option<Bearing>,
    times: TimeCoverage,
) -> ObjectDetail {
    ObjectDetail::Account {
        id: id.to_owned(),
        version: 1,
        label: label.to_owned(),
        origin: badge("◉", "testimony", "neutral"),
        anchor: anchor.to_owned(),
        assessment: badge("○", "unsupported", "neutral"),
        review: badge("◻", "unreviewed", "neutral"),
        conflicts,
        source_record,
        bearing,
        times,
        timeline_focus: object_focus(id),
        your_notes: Vec::new(),
    }
}

#[must_use]
pub fn object(object_id: &str) -> Option<ObjectDetail> {
    match object_id {
        "FIXTURE-ACC-A" => Some(account(
            "FIXTURE-ACC-A",
            "Account A · observation",
            "FIXTURE-DOC-0007 · v2 · p3.¶3",
            vec![badge("✕", "1 · account C", "bad")],
            vec![
                kv("Observer", "Eastern road (reported)", false),
                kv(
                    "Event time",
                    "21:40 ±15 min · 1994-03-14 · America/Denver · reported local",
                    true,
                ),
                kv("Received", "1994-03-20 (log date)", true),
                kv("Direction", "NW → SE · ±15° · \"approx.\"", true),
                kv("Duration", "≈ 8 min (reported)", true),
            ],
            Some(Bearing {
                degrees: 135.0,
                plus_minus_degrees: 15.0,
                text: "NW → SE · ±15° · \"approx.\"",
            }),
            TimeCoverage {
                event: Coverage::Approximate,
                received: Coverage::Recorded,
                position: Coverage::Recorded,
            },
        )),
        "FIXTURE-ACC-B" => Some(account(
            "FIXTURE-ACC-B",
            "Account B · observation",
            "FIXTURE-DOC-0007 · v2 · p4.¶1 (page unreadable)",
            Vec::new(),
            vec![
                kv("Observer", "— unavailable", false),
                kv("Event time", "— unavailable", true),
                kv("Received", "1994-03-22 (log date)", true),
                kv("Direction", "— unavailable", true),
            ],
            None,
            TimeCoverage {
                event: Coverage::NotRecorded,
                received: Coverage::Recorded,
                position: Coverage::NotRecorded,
            },
        )),
        "FIXTURE-ACC-C" => Some(account(
            "FIXTURE-ACC-C",
            "Account C · observation",
            "FIXTURE-DOC-0007 · v2 · p5.¶1",
            vec![badge("✕", "1 · account A", "bad")],
            vec![
                kv("Observer", "\"east of the ridge\" (region only)", false),
                kv("Event time", "— unavailable", true),
                kv("Received", "1994-04-02 (log date)", true),
                kv("Direction", "— no bearing", true),
                kv("Count", "2 lights (reported)", true),
            ],
            None,
            TimeCoverage {
                event: Coverage::NotRecorded,
                received: Coverage::Recorded,
                position: Coverage::RegionOnly,
            },
        )),
        DOC_ID => Some(ObjectDetail::Source {
            id: DOC_ID.to_owned(),
            version: DOC_VERSION,
            title: "County incident log, March 1994 (synthetic)".to_owned(),
            origin: badge("▣", "source", "neutral"),
            availability: badge("●", "available", "ok"),
            health: badge("●", "available · fixture", "ok"),
            source_record: vec![
                kv("Source id", DOC_ID, true),
                kv("Version", "v2 (supersedes v1: 2 pages re-scanned)", true),
                kv("Hash", "sha256 · 0f1x…fixture (not a real digest)", true),
                kv("Retrieved", "fixture · step 2", true),
                kv("Publication", "1994-04 (log cover date)", true),
                kv(
                    "Use conditions",
                    "fixture · no redistribution question arises",
                    false,
                ),
            ],
            pages: 9,
            unreadable_pages: vec![4, 8],
            lineage: vec![
                "v1 · initial scan · 9 pages · 4 unreadable".to_owned(),
                "v2 · pages 2 and 6 re-scanned · 2 unreadable".to_owned(),
            ],
            your_notes: Vec::new(),
        }),
        "FIXTURE-HYP-0001" => Some(ObjectDetail::Hypothesis {
            id: "FIXTURE-HYP-0001".to_owned(),
            version: 2,
            text: "One object moving north-west to south-east, seen from three places".to_owned(),
            origin: badge("◇", "inference", "warn"),
            supporting: vec!["FIXTURE-ACC-A · direction NW → SE".to_owned()],
            contradicting: vec!["FIXTURE-ACC-C · two lights, no bearing".to_owned()],
            discriminating: "A position or bearing for account B".to_owned(),
        }),
        "FIXTURE-CLM-0001" => Some(ObjectDetail::Claim {
            id: "FIXTURE-CLM-0001".to_owned(),
            version: 1,
            proposition: "\"The light moved from the north-west toward the south-east.\""
                .to_owned(),
            claimant: "Account A (as logged)".to_owned(),
            anchor: "FIXTURE-DOC-0007 · v2 · p3.¶3".to_owned(),
            assessment: badge("○", "unsupported", "neutral"),
            review: badge("◻", "unreviewed", "neutral"),
            supporting: Vec::new(),
            contradicting: Vec::new(),
        }),
        "FIXTURE-CLM-0002" => Some(ObjectDetail::Claim {
            id: "FIXTURE-CLM-0002".to_owned(),
            version: 1,
            proposition: "\"Two lights were seen east of the ridge.\"".to_owned(),
            claimant: "Account C (as logged)".to_owned(),
            anchor: "FIXTURE-DOC-0007 · v2 · p5.¶1".to_owned(),
            assessment: badge("…", "unresolved", "neutral"),
            review: badge("◻", "unreviewed", "neutral"),
            supporting: Vec::new(),
            contradicting: Vec::new(),
        }),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentRow {
    pub id: String,
    pub version: u32,
    pub title: String,
    pub origin: Badge,
    pub publication: String,
    pub availability: Badge,
    pub extraction: String,
    pub health: Badge,
    pub last_acquired: String,
}

#[must_use]
pub fn documents() -> Vec<DocumentRow> {
    vec![DocumentRow {
        id: DOC_ID.to_owned(),
        version: DOC_VERSION,
        title: "County incident log, March 1994 (synthetic)".to_owned(),
        origin: badge("▣", "source", "neutral"),
        publication: "1994-04 (log cover date)".to_owned(),
        availability: badge("●", "available", "ok"),
        extraction: "7 of 9 pages readable · pages 4 and 8 unreadable".to_owned(),
        health: badge("●", "available · fixture", "ok"),
        last_acquired: "fixture · step 2".to_owned(),
    }]
}

struct Passage {
    page: u32,
    paragraph: u32,
    text: &'static str,
}

const PASSAGES: &[Passage] = &[
    Passage {
        page: 1,
        paragraph: 1,
        text: "Incident log, March 1994. Entries are transcribed from the duty book.",
    },
    Passage {
        page: 2,
        paragraph: 1,
        text: "14 March. Routine entries. No calls after 19:00.",
    },
    Passage {
        page: 3,
        paragraph: 3,
        text: "Caller (A) on the eastern road reports a single light moving from the north-west toward the south-east over about eight minutes, around 21:40, give or take a quarter hour.",
    },
    Passage {
        page: 5,
        paragraph: 1,
        text: "Letter received 2 April (C): two lights seen east of the ridge on the night of the 14th. No time or direction given.",
    },
    Passage {
        page: 6,
        paragraph: 2,
        text: "Note received 22 March (B): the writer saw a light that night but does not say from where.",
    },
    Passage {
        page: 7,
        paragraph: 1,
        text: "No further entries relating to the 14th.",
    },
    Passage {
        page: 9,
        paragraph: 1,
        text: "End of March entries.",
    },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub doc_id: String,
    pub version: u32,
    pub anchor: String,
    pub text: String,
    pub match_start: usize,
    pub match_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub query: String,
    pub scope: String,
    pub mode: String,
    pub hits: Vec<SearchHit>,
    pub searched_passages: usize,
    pub searched_documents: usize,
    pub coverage: String,
    pub outcome: String,
}

/// Case-insensitive exact substring search over fixture passages. This is a
/// stand-in for the bounded kernels in `crates/oia-documents`, not a port.
#[must_use]
pub fn search(query: &str, scope: &str) -> SearchResult {
    let needle: String = query
        .trim()
        .chars()
        .take(MAX_QUERY_CHARS)
        .collect::<String>()
        .to_lowercase();
    let mut hits = Vec::new();
    if !needle.is_empty() {
        for passage in PASSAGES {
            let hay = passage.text.to_lowercase();
            if let Some(start) = hay.find(&needle) {
                hits.push(SearchHit {
                    doc_id: DOC_ID.to_owned(),
                    version: DOC_VERSION,
                    anchor: format!("p{}.¶{}", passage.page, passage.paragraph),
                    text: passage.text.to_owned(),
                    match_start: start,
                    match_end: start + needle.len(),
                });
            }
        }
    }
    let outcome = if hits.is_empty() { "empty" } else { "success" }.to_owned();
    SearchResult {
        query: query.trim().chars().take(MAX_QUERY_CHARS).collect(),
        scope: scope.to_owned(),
        mode: "exact".to_owned(),
        hits,
        searched_passages: PASSAGES.len(),
        searched_documents: 1,
        coverage: format!(
            "Searched {} indexed passages from 1 document. Pages 4 and 8 not indexed: unreadable.",
            PASSAGES.len()
        ),
        outcome,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Statement {
    pub id: String,
    pub section: &'static str,
    pub text: String,
    pub citation: Option<String>,
    /// human | generated
    pub origin: &'static str,
    pub reviewed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportDraft {
    pub revision: u32,
    pub statements: Vec<Statement>,
    pub uncited: u32,
}

#[must_use]
pub fn report_draft() -> ReportDraft {
    let statements = vec![
        Statement {
            id: "FIXTURE-STMT-01".to_owned(),
            section: "Question and scope",
            text: "Three accounts logged in March 1994 describe lights over the valley on the night of the 14th.".to_owned(),
            citation: Some("FIXTURE-DOC-0007 · v2 · p3.¶3".to_owned()),
            origin: "human",
            reviewed: true,
        },
        Statement {
            id: "FIXTURE-STMT-02".to_owned(),
            section: "Supporting and conflicting evidence",
            text: "Account A gives a direction of travel; accounts B and C do not.".to_owned(),
            citation: Some("FIXTURE-DOC-0007 · v2 · p3.¶3; p5.¶1; p6.¶2".to_owned()),
            origin: "human",
            reviewed: false,
        },
        Statement {
            id: "FIXTURE-STMT-03".to_owned(),
            section: "Supporting and conflicting evidence",
            text: "The two lights in account C may be the same object seen twice.".to_owned(),
            citation: None,
            origin: "generated",
            reviewed: false,
        },
        Statement {
            id: "FIXTURE-STMT-04".to_owned(),
            section: "Open questions",
            text: "Account B's position is not recorded.".to_owned(),
            citation: None,
            origin: "human",
            reviewed: false,
        },
    ];
    let uncited = u32::try_from(statements.iter().filter(|s| s.citation.is_none()).count())
        .unwrap_or(u32::MAX);
    ReportDraft {
        revision: 1,
        statements,
        uncited,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FormatRow {
    pub name: &'static str,
    /// implemented | planned
    pub status: &'static str,
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportPreview {
    pub formats: Vec<FormatRow>,
    pub omissions: Vec<&'static str>,
    pub note: &'static str,
}

#[must_use]
pub fn export_preview() -> ExportPreview {
    ExportPreview {
        formats: vec![
            FormatRow {
                name: "Markdown report",
                status: "planned",
                note: "ADR 0006 selects it; no exporter exists in this spike",
            },
            FormatRow {
                name: "Self-contained HTML",
                status: "planned",
                note: "ADR 0006 selects it; no exporter exists in this spike",
            },
            FormatRow {
                name: "JSON directory bundle",
                status: "planned",
                note: "ADR 0006 selects it; no exporter exists in this spike",
            },
            FormatRow {
                name: "PDF",
                status: "planned",
                note: "After render QA (ADR 0006)",
            },
        ],
        omissions: vec![
            "Pages 4 and 8 of FIXTURE-DOC-0007: unreadable, no text to include",
            "FIXTURE-STMT-03: generated and unreviewed; would be labelled, not dropped",
        ],
        note: "Nothing is written by this spike. The write action stays disabled.",
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TimeMark {
    pub account_id: String,
    pub label: &'static str,
    /// event | capture | publication | revision | retrieval | analysis
    pub kind: &'static str,
    pub glyph: &'static str,
    /// `None` is drawn as an explicit "no time" row.
    pub range_min: Option<(u32, u32)>,
    pub note: &'static str,
}

#[must_use]
pub fn timeline_marks() -> Vec<TimeMark> {
    vec![
        TimeMark {
            account_id: "FIXTURE-ACC-A".to_owned(),
            label: "A",
            kind: "event",
            glyph: "◆",
            range_min: Some((21 * 60 + 25, 21 * 60 + 55)),
            note: "event ≈ 21:40 ±15",
        },
        TimeMark {
            account_id: "FIXTURE-ACC-B".to_owned(),
            label: "B",
            kind: "event",
            glyph: "◆",
            range_min: None,
            note: "no event time",
        },
        TimeMark {
            account_id: "FIXTURE-ACC-C".to_owned(),
            label: "C",
            kind: "event",
            glyph: "◆",
            range_min: None,
            note: "no event time",
        },
    ]
}
