//! Builds or inspects an evidence ledger (`#10`).
//!
//! ```text
//! cargo run -p oia-domain --example evidence-ledger --locked -- fixture
//! cargo run -p oia-domain --example evidence-ledger --locked -- inspect fixtures/domain/evidence-ledger-correction.json
//! ```
//!
//! `fixture` prints the synthetic correction scenario committed as
//! `fixtures/domain/evidence-ledger-correction.json`. `inspect` imports a ledger document
//! under default limits and prints each version with its lineage labels and reassessment flags.
//! Exit code `0` means the document was accepted and `1` means it was rejected.

use std::collections::BTreeMap;
use std::error::Error;

use oia_domain::evidence::{
    AttributeName, AttributeValue, Availability, ClaimAssessment, ClaimFields, EvidenceLedger,
    FieldValue, LedgerEntry, LedgerLimits, ObjectType, OriginClass, Provenance, RecordFields,
    Relation, RelationKind, ReviewStatus, Revision, RevisionReason, Scope, UnknownReason,
};
use oia_domain::{ActorId, OpaqueId, VersionId};

/// 2026-11-03T12:00:00Z, the synthetic OIA creation time. Event times stay in attributes.
const T0: i64 = 1_793_707_200_000;
const MINUTE: i64 = 60_000;

type Attributes = BTreeMap<AttributeName, FieldValue<AttributeValue>>;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd] if cmd == "fixture" => {
            print!("{}", correction_scenario()?.to_json()?);
            Ok(())
        }
        [cmd, path] if cmd == "inspect" => inspect(path),
        _ => Err("usage: evidence-ledger fixture | evidence-ledger inspect PATH".into()),
    }
}

fn inspect(path: &str) -> Result<(), Box<dyn Error>> {
    let text = std::fs::read_to_string(path)?;
    let ledger = EvidenceLedger::from_json(&text, LedgerLimits::default())?;
    println!("accepted {} entries", ledger.entries().len());
    for entry in ledger.entries() {
        match entry {
            LedgerEntry::Record(version) => {
                let Some(record) = ledger.version(version) else {
                    continue;
                };
                let f = record.fields();
                let labels = record.labels();
                let state = match ledger.successor(version) {
                    Some(next) => format!("superseded by {next}"),
                    None => "current".to_owned(),
                };
                println!(
                    "record {version} [{} / {}] {state}; simulated={} illustrative={}",
                    f.object_type, f.origin, labels.simulated, labels.illustrative
                );
                for flag in ledger.reassessment_flags(version) {
                    println!(
                        "  needs_reassessment: {} -> {} ({}), via {}, {} hop(s)",
                        flag.replaced, flag.replacement, flag.reason, flag.via, flag.hops
                    );
                }
            }
            LedgerEntry::Relation(id) => {
                if let Some(r) = ledger.relation(id) {
                    println!(
                        "relation {id}: {} (dependent {}, upstream {}) [{}]",
                        r.kind, r.dependent, r.upstream, r.review
                    );
                }
            }
        }
    }
    Ok(())
}

fn id(raw: &str) -> Result<OpaqueId, Box<dyn Error>> {
    Ok(OpaqueId::new(raw)?)
}

fn ver(raw: &str) -> Result<VersionId, Box<dyn Error>> {
    Ok(VersionId::new(raw)?)
}

fn attrs(pairs: Vec<(&str, FieldValue<AttributeValue>)>) -> Result<Attributes, Box<dyn Error>> {
    let mut map = BTreeMap::new();
    for (name, value) in pairs {
        map.insert(AttributeName::new(name)?, value);
    }
    Ok(map)
}

fn text(value: &str) -> FieldValue<AttributeValue> {
    FieldValue::Known(AttributeValue::Text(value.to_owned()))
}

fn derived(method: &str, inputs: &[&str]) -> Result<Provenance, Box<dyn Error>> {
    Ok(Provenance::Derived {
        method: id(method)?,
        method_version: ver("v1")?,
        inputs: inputs.iter().map(|v| ver(v)).collect::<Result<_, _>>()?,
    })
}

struct Base {
    case: OpaqueId,
    analyst: ActorId,
}

impl Base {
    #[allow(clippy::too_many_arguments)]
    fn record(
        &self,
        object: &str,
        version: &str,
        object_type: ObjectType,
        origin: OriginClass,
        minutes: i64,
        provenance: Provenance,
        attributes: Attributes,
    ) -> Result<RecordFields, Box<dyn Error>> {
        Ok(RecordFields {
            id: id(object)?,
            version: ver(version)?,
            object_type,
            origin,
            created_at_ms: T0 + minutes * MINUTE,
            created_by: self.analyst.clone(),
            scope: Scope::Case(self.case.clone()),
            access_policies: vec![id("policy:case-members")?],
            provenance,
            review: ReviewStatus::Unreviewed,
            availability: Availability::Available,
            attributes,
            claim: None,
        })
    }

    fn relation(
        &self,
        relation: &str,
        kind: RelationKind,
        dependent: &str,
        upstream: &str,
        minutes: i64,
        rationale: FieldValue<String>,
    ) -> Result<Relation, Box<dyn Error>> {
        Ok(Relation {
            id: id(relation)?,
            kind,
            dependent: ver(dependent)?,
            upstream: ver(upstream)?,
            created_at_ms: T0 + minutes * MINUTE,
            created_by: self.analyst.clone(),
            review: ReviewStatus::Unreviewed,
            evidence: Vec::new(),
            rationale,
        })
    }
}

/// Synthetic A03-style correction: a newspaper article, a witness account and the records
/// derived from them, followed by a source correction of the article.
fn correction_scenario() -> Result<EvidenceLedger, Box<dyn Error>> {
    let base = Base {
        case: id("case:reservoir-lights")?,
        analyst: ActorId::new("actor:analyst-1")?,
    };
    let mut ledger = EvidenceLedger::new(LedgerLimits::default());

    let article_v1 = base.record(
        "rec:gazette-article",
        "rec:gazette-article:v1",
        ObjectType::SourceRecord,
        OriginClass::AcquiredSourceMaterial,
        0,
        Provenance::Acquired {
            source: id("src:county-gazette")?,
        },
        attrs(vec![
            ("headline", text("Lights over the reservoir")),
            (
                "body_excerpt",
                text("Residents reported two lights over the reservoir at about 9 p.m.\r\nThe lights moved north."),
            ),
            ("byline", FieldValue::Unknown(UnknownReason::NotRecordedBySource)),
            ("page", FieldValue::Known(AttributeValue::Integer(3))),
        ])?,
    )?;
    ledger.issue(article_v1)?;

    let account = base.record(
        "acct:witness-1",
        "acct:witness-1:v1",
        ObjectType::Observation,
        OriginClass::UserTestimony,
        5,
        Provenance::Manual,
        attrs(vec![
            (
                "narrative",
                text("I saw two steady lights heading north over the water."),
            ),
            ("duration", FieldValue::Unknown(UnknownReason::NotReported)),
            ("sound", FieldValue::Unknown(UnknownReason::NotReported)),
        ])?,
    )?;
    ledger.issue(account)?;

    let observation = base.record(
        "obs:light-count",
        "obs:light-count:v1",
        ObjectType::Observation,
        OriginClass::NormalizedObservation,
        10,
        derived(
            "method:manual-normalization",
            &["rec:gazette-article:v1", "acct:witness-1:v1"],
        )?,
        attrs(vec![
            ("light_count", FieldValue::Known(AttributeValue::Integer(2))),
            ("direction", text("north")),
            (
                "altitude",
                FieldValue::Unknown(UnknownReason::NotRecordedBySource),
            ),
        ])?,
    )?;
    ledger.issue(observation)?;

    let mut claim = base.record(
        "claim:gazette-time",
        "claim:gazette-time:v1",
        ObjectType::Claim,
        OriginClass::ExtractedClaim,
        12,
        derived("method:claim-extraction", &["rec:gazette-article:v1"])?,
        BTreeMap::new(),
    )?;
    claim.claim = Some(ClaimFields {
        proposition: "Two lights were seen over the reservoir at about 9 p.m.".to_owned(),
        claimant: FieldValue::Known("County Gazette".to_owned()),
        assessment: ClaimAssessment::Unresolved,
    });
    ledger.issue(claim)?;

    let inference = base.record(
        "inf:aircraft-hypothesis",
        "inf:aircraft-hypothesis:v1",
        ObjectType::AnalysisRun,
        OriginClass::AnalyticalInference,
        20,
        derived("method:flight-overlap", &["obs:light-count:v1"])?,
        attrs(vec![
            (
                "hypothesis",
                text("Two aircraft on approach could match the reported lights."),
            ),
            (
                "flight_data",
                FieldValue::Unknown(UnknownReason::NotYetProcessed),
            ),
        ])?,
    )?;
    ledger.issue(inference)?;

    let simulation = base.record(
        "sim:two-light-replay",
        "sim:two-light-replay:v1",
        ObjectType::AnalysisRun,
        OriginClass::Simulation,
        25,
        Provenance::Manual,
        attrs(vec![(
            "scenario",
            text("Two lights moving north at 2 deg/s"),
        )])?,
    )?;
    ledger.issue(simulation)?;

    let comparison = base.record(
        "inf:replay-comparison",
        "inf:replay-comparison:v1",
        ObjectType::AnalysisRun,
        OriginClass::AnalyticalInference,
        30,
        derived(
            "method:replay-comparison",
            &["inf:aircraft-hypothesis:v1", "sim:two-light-replay:v1"],
        )?,
        attrs(vec![(
            "summary",
            text("The simulated motion is compatible with the hypothesis."),
        )])?,
    )?;
    ledger.issue(comparison)?;

    ledger.relate(base.relation(
        "rel:count-supports-claim",
        RelationKind::Supports,
        "claim:gazette-time:v1",
        "obs:light-count:v1",
        35,
        FieldValue::Known("The normalized count matches the article wording.".to_owned()),
    )?)?;
    ledger.relate(base.relation(
        "rel:account-article-event",
        RelationKind::PossibleSameEvent,
        "acct:witness-1:v1",
        "rec:gazette-article:v1",
        36,
        FieldValue::Unknown(UnknownReason::NotYetProcessed),
    )?)?;

    let article_v2 = base.record(
        "rec:gazette-article",
        "rec:gazette-article:v2",
        ObjectType::SourceRecord,
        OriginClass::AcquiredSourceMaterial,
        60,
        Provenance::Acquired {
            source: id("src:county-gazette")?,
        },
        attrs(vec![
            ("headline", text("Lights over the reservoir")),
            (
                "body_excerpt",
                text("Residents reported two lights over the reservoir at about 10 p.m.\r\nThe lights moved north."),
            ),
            ("byline", FieldValue::Unknown(UnknownReason::NotRecordedBySource)),
            ("page", FieldValue::Known(AttributeValue::Integer(3))),
        ])?,
    )?;
    ledger.revise(
        article_v2,
        &ver("rec:gazette-article:v1")?,
        Revision {
            reason: RevisionReason::SourceCorrection,
            rationale: "The Gazette corrected the reported time from about 9 p.m. to about 10 p.m."
                .to_owned(),
        },
    )?;
    Ok(ledger)
}
