//! Contract checks for the canonical evidence ledger (`#10`, architecture §2, A03/A07 subsets).
//!
//! Each test covers a risk that ordinary execution would not reveal:
//! 1. The committed fixture round-trips byte-for-byte, keeps the original version, its
//!    verbatim text and unknown values, and flags exactly the dependents of a correction.
//! 2. Generated material cannot be promoted, lineage labels cannot be shed through a later
//!    derivation edge, derivation cycles and stale revisions are rejected, and a rejected
//!    operation leaves the ledger unchanged.
//! 3. Import is strict: oversized, unknown, duplicate, miscoded and rule-violating input is
//!    refused rather than repaired.

use std::collections::BTreeMap;
use std::error::Error;

use oia_domain::evidence::{
    AttributeName, AttributeValue, Availability, CodecError, EvidenceError, EvidenceLedger,
    FieldValue, LedgerLimits, MAX_LEDGER_JSON_BYTES, ObjectType, OriginClass, Provenance,
    RecordFields, Relation, RelationKind, ReviewStatus, Revision, RevisionReason, Scope,
    UnknownReason,
};
use oia_domain::{ActorId, OpaqueId, VersionId};

type TestResult = Result<(), Box<dyn Error>>;

const FIXTURE: &str = include_str!("../../../fixtures/domain/evidence-ledger-correction.json");

/// Git may check the fixture out with CRLF line endings. Canonical output never contains a raw
/// CR because JSON escapes CR inside strings, so normalizing line endings is lossless here.
fn fixture() -> String {
    FIXTURE.replace("\r\n", "\n")
}

fn v(raw: &str) -> Result<VersionId, Box<dyn Error>> {
    Ok(VersionId::new(raw)?)
}

#[test]
fn correction_fixture_round_trips_and_flags_only_dependents() -> TestResult {
    let fixture = fixture();
    let ledger = EvidenceLedger::from_json(&fixture, LedgerLimits::default())?;
    assert_eq!(ledger.to_json()?, fixture, "canonical re-encoding changed");

    // The corrected original remains, unchanged and linked to its replacement.
    let v1 = ledger
        .version(&v("rec:gazette-article:v1")?)
        .ok_or("v1 missing")?;
    let v2 = ledger
        .version(&v("rec:gazette-article:v2")?)
        .ok_or("v2 missing")?;
    let excerpt = AttributeName::new("body_excerpt")?;
    assert_eq!(
        v1.fields().attributes.get(&excerpt),
        Some(&FieldValue::Known(AttributeValue::Text(
            "Residents reported two lights over the reservoir at about 9 p.m.\r\nThe lights moved north."
                .to_owned()
        ))),
        "original wording, including CRLF, must survive"
    );
    assert_eq!(ledger.successor(v1.version()), Some(v2.version()));
    assert_eq!(v2.predecessor(), Some(v1.version()));
    assert_eq!(
        v2.revision().map(|r| r.reason),
        Some(RevisionReason::SourceCorrection)
    );

    // Unknown values keep their reasons rather than becoming defaults.
    let byline = AttributeName::new("byline")?;
    assert_eq!(
        v2.fields().attributes.get(&byline),
        Some(&FieldValue::Unknown(UnknownReason::NotRecordedBySource))
    );
    let account = ledger
        .version(&v("acct:witness-1:v1")?)
        .ok_or("account missing")?;
    assert_eq!(
        account
            .fields()
            .attributes
            .get(&AttributeName::new("duration")?),
        Some(&FieldValue::Unknown(UnknownReason::NotReported))
    );

    // Exactly the propagating dependents are flagged, each once, with the first path found.
    let expected = [
        ("obs:light-count:v1", "rec:gazette-article:v1", 1),
        ("claim:gazette-time:v1", "rec:gazette-article:v1", 1),
        ("inf:aircraft-hypothesis:v1", "obs:light-count:v1", 2),
        ("inf:replay-comparison:v1", "inf:aircraft-hypothesis:v1", 3),
    ];
    for (version, via, hops) in expected {
        let flags = ledger.reassessment_flags(&v(version)?);
        assert_eq!(flags.len(), 1, "{version} flags: {flags:?}");
        let flag = &flags[0];
        assert_eq!(&flag.replaced, v1.version());
        assert_eq!(&flag.replacement, v2.version());
        assert_eq!(flag.reason, RevisionReason::SourceCorrection);
        assert_eq!(flag.via, v(via)?);
        assert_eq!(flag.hops, hops, "{version}");
    }
    // The witness account is only a possible-same-event peer; the simulation is unrelated.
    for version in [
        "acct:witness-1:v1",
        "sim:two-light-replay:v1",
        "rec:gazette-article:v1",
        "rec:gazette-article:v2",
    ] {
        assert!(!ledger.needs_reassessment(&v(version)?), "{version}");
    }

    // Simulation lineage is inherited by the comparison but not by its sibling inference.
    let comparison = ledger
        .version(&v("inf:replay-comparison:v1")?)
        .ok_or("comparison missing")?;
    assert!(comparison.labels().simulated);
    let hypothesis = ledger
        .version(&v("inf:aircraft-hypothesis:v1")?)
        .ok_or("hypothesis missing")?;
    assert!(!hypothesis.labels().simulated);
    Ok(())
}

fn record(
    object: &str,
    version: &str,
    origin: OriginClass,
    provenance: Provenance,
) -> Result<RecordFields, Box<dyn Error>> {
    Ok(RecordFields {
        id: OpaqueId::new(object)?,
        version: v(version)?,
        object_type: ObjectType::Observation,
        origin,
        created_at_ms: 1_793_707_200_000,
        created_by: ActorId::new("actor:test")?,
        scope: Scope::Library,
        access_policies: Vec::new(),
        provenance,
        review: ReviewStatus::Unreviewed,
        availability: Availability::Available,
        attributes: BTreeMap::new(),
        claim: None,
    })
}

fn derived(inputs: &[&str]) -> Result<Provenance, Box<dyn Error>> {
    Ok(Provenance::Derived {
        method: OpaqueId::new("method:test")?,
        method_version: v("v1")?,
        inputs: inputs.iter().map(|i| v(i)).collect::<Result<_, _>>()?,
    })
}

fn relation(
    id: &str,
    kind: RelationKind,
    dependent: &str,
    upstream: &str,
) -> Result<Relation, Box<dyn Error>> {
    Ok(Relation {
        id: OpaqueId::new(id)?,
        kind,
        dependent: v(dependent)?,
        upstream: v(upstream)?,
        created_at_ms: 1_793_707_200_000,
        created_by: ActorId::new("actor:test")?,
        review: ReviewStatus::Unreviewed,
        evidence: Vec::new(),
        rationale: FieldValue::Unknown(UnknownReason::NotYetProcessed),
    })
}

#[test]
fn derivation_rules_reject_promotion_cycles_and_stale_revisions() -> TestResult {
    use OriginClass::*;
    let mut ledger = EvidenceLedger::new(LedgerLimits::default());
    let source = Provenance::Acquired {
        source: OpaqueId::new("src:test")?,
    };
    ledger.issue(record(
        "src",
        "src:v1",
        AcquiredSourceMaterial,
        source.clone(),
    )?)?;
    ledger.issue(record("sim", "sim:v1", Simulation, Provenance::Manual)?)?;
    ledger.issue(record(
        "inf",
        "inf:v1",
        AnalyticalInference,
        derived(&["src:v1"])?,
    )?)?;
    ledger.issue(record(
        "inf2",
        "inf2:v1",
        AnalyticalInference,
        derived(&["inf:v1"])?,
    )?)?;
    let snapshot = ledger.clone();

    // An inference cannot become an observation, directly or through a later edge.
    let promoted = record(
        "obs",
        "obs:v1",
        NormalizedObservation,
        derived(&["inf:v1"])?,
    )?;
    assert!(matches!(
        ledger.issue(promoted),
        Err(EvidenceError::OriginPromotion {
            input_origin: AnalyticalInference,
            ..
        })
    ));
    // A derivation edge cannot attach simulation lineage to an issued, unlabeled version.
    assert!(matches!(
        ledger.relate(relation(
            "rel:sim",
            RelationKind::DerivedFrom,
            "inf:v1",
            "sim:v1"
        )?),
        Err(EvidenceError::LabelNotInherited { .. })
    ));
    // inf2 derives from inf, so inf cannot also derive from inf2.
    assert!(matches!(
        ledger.relate(relation(
            "rel:cycle",
            RelationKind::DerivedFrom,
            "inf:v1",
            "inf2:v1"
        )?),
        Err(EvidenceError::DerivationCycle { .. })
    ));
    assert_eq!(
        ledger, snapshot,
        "rejected operations must not change the ledger"
    );

    // Lineage edges may form citation cycles.
    ledger.relate(relation(
        "rel:q1",
        RelationKind::Quotes,
        "inf2:v1",
        "inf:v1",
    )?)?;
    ledger.relate(relation(
        "rel:q2",
        RelationKind::Quotes,
        "inf:v1",
        "inf2:v1",
    )?)?;

    let correction = || Revision {
        reason: RevisionReason::SourceCorrection,
        rationale: "Publisher correction".to_owned(),
    };
    let report = ledger.revise(
        record("src", "src:v2", AcquiredSourceMaterial, source.clone())?,
        &v("src:v1")?,
        correction(),
    )?;
    assert_eq!(report.flagged, vec![v("inf:v1")?, v("inf2:v1")?]);

    let snapshot = ledger.clone();
    assert!(matches!(
        ledger.revise(
            record("src", "src:v3", AcquiredSourceMaterial, source)?,
            &v("src:v1")?,
            correction()
        ),
        Err(EvidenceError::StaleRevision { .. })
    ));
    assert!(matches!(
        ledger.revise(
            record("src", "src:v3", UserTestimony, Provenance::Manual)?,
            &v("src:v2")?,
            correction()
        ),
        Err(EvidenceError::ImmutableFieldChanged { field: "origin" })
    ));
    assert_eq!(
        ledger, snapshot,
        "rejected revisions must not change the ledger"
    );

    // Deriving from the superseded original is allowed but immediately flagged.
    ledger.issue(record(
        "late",
        "late:v1",
        AnalyticalInference,
        derived(&["src:v1"])?,
    )?)?;
    let flags = ledger.reassessment_flags(&v("late:v1")?);
    assert_eq!(flags.len(), 1);
    assert_eq!(flags[0].replacement, v("src:v2")?);
    Ok(())
}

#[test]
fn json_import_is_strict() -> TestResult {
    let fixture = fixture();
    let limits = LedgerLimits::default();

    let oversized = " ".repeat(MAX_LEDGER_JSON_BYTES + 1);
    assert!(matches!(
        EvidenceLedger::from_json(&oversized, limits),
        Err(CodecError::Oversized { .. })
    ));

    let unknown_field = fixture.replacen("\"format\"", "\"extra\": true,\n  \"format\"", 1);
    assert!(matches!(
        EvidenceLedger::from_json(&unknown_field, limits),
        Err(CodecError::Syntax(_))
    ));

    let duplicate_field = fixture.replacen(
        "\"schema_version\": 1,",
        "\"schema_version\": 1,\n  \"schema_version\": 1,",
        1,
    );
    assert!(matches!(
        EvidenceLedger::from_json(&duplicate_field, limits),
        Err(CodecError::Syntax(_))
    ));

    let unknown_code = fixture.replacen("\"user_testimony\"", "\"eyewitness\"", 1);
    assert!(matches!(
        EvidenceLedger::from_json(&unknown_code, limits),
        Err(CodecError::UnknownCode {
            field: "origin",
            ..
        })
    ));

    // Relabelling the simulation-derived comparison as an observation must fail on replay.
    let marker = "\"version\": \"inf:replay-comparison:v1\"";
    let at = fixture.find(marker).ok_or("marker missing")?;
    let (head, tail) = fixture.split_at(at);
    let promoted = format!(
        "{head}{}",
        tail.replacen(
            "\"origin\": \"analytical_inference\"",
            "\"origin\": \"normalized_observation\"",
            1
        )
    );
    assert!(matches!(
        EvidenceLedger::from_json(&promoted, limits),
        Err(CodecError::Rejected {
            source: EvidenceError::OriginPromotion { .. },
            ..
        })
    ));
    Ok(())
}
