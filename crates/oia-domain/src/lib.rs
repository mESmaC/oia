//! `oia-domain` — Core domain identifiers, versioned evidence envelopes, claim/review states,
//! content hashes, resource-bounded JSON, and provenance graphs for Open Intelligence Agency.
//!
//! This crate has zero dependencies on UI toolkits, HTTP clients, database engines, model
//! services, or vendor SDKs (`docs/architecture.md` Section 1.2).

pub mod codec;
pub mod graph;
pub mod ids;
pub mod json;
pub mod records;
pub mod sha256;

pub use graph::{EvidenceGraph, RelationEdge, RelationKind};
pub use ids::{ActorId, IdError, MAX_ID_LEN, OpaqueId, SchemaVersion, VersionId};
pub use json::{JsonBudget, JsonError, JsonValue};
pub use records::{
    Annotation, Artifact, AvailabilityState, CitationAnchor, Claim, ClaimAssessment, ClaimNature,
    Document, DocumentReleaseKind, DomainError, DomainObject, DomainObjectType, Event,
    ExtractionCoverage, Hypothesis, Investigation, ObjectEnvelope, Observation, OmissionReason,
    OriginClass, ProvenanceRecord, ReassessmentState, RecordedValue, Report, ReportBlock,
    ReportPublicationState, ResearchLead, RetainedContent, ReuseConditions,
    ReversibleClusterDecision, Review, ReviewStatus, SavedQuery, Source, SourceHealthState,
    SourceRecord, WorkspaceVisibility,
};
pub use sha256::{ContentHash, HashParseError};

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies FIPS 180-4 SHA-256 against NIST test vectors and checks that identical bytes
    /// under two publishers remain distinct `SourceRecord` versions.
    #[test]
    fn sha256_nist_vectors_and_distinct_publisher_records() -> Result<(), DomainError> {
        let empty = ContentHash::compute(b"");
        assert_eq!(
            empty.to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let abc = ContentHash::compute(b"abc");
        assert_eq!(
            abc.to_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(abc.verify_bytes(b"abc"));
        assert!(!abc.verify_bytes(b"abd"));

        let actor = ActorId::new("ingest_worker")?;
        let shared_hash = ContentHash::compute(b"shared wire bulletin payload");
        let rec_a = SourceRecord {
            envelope: ObjectEnvelope {
                id: OpaqueId::new("sr_wire_ap")?,
                version_id: VersionId::new("ver_sr_wire_ap_1")?,
                schema_version: SchemaVersion::V1,
                object_type: DomainObjectType::SourceRecord,
                created_at_ms: 1_700_000_000_000,
                created_by: actor.clone(),
                visibility: WorkspaceVisibility::LibraryPublic,
                supersedes: None,
                provenance: ProvenanceRecord {
                    source_version_ids: Vec::new(),
                    declared_origin_note: Some("Acquired from publisher A".to_owned()),
                    processing_history: vec!["sha256_verified".to_owned()],
                },
                origin_class: OriginClass::AcquiredSourceMaterial,
                review_status: ReviewStatus::Unreviewed,
                availability: AvailabilityState::Available,
                reassessment: ReassessmentState::Current,
            },
            source_id: OpaqueId::new("src_publisher_a")?,
            upstream_version: "2026-11-01-v1".to_owned(),
            acquisition_attempt_id: OpaqueId::new("acq_1")?,
            source_locator: "https://publisher-a.example/wire/1".to_owned(),
            source_event_time_ms: Some(1_699_999_000_000),
            public_availability_time_ms: Some(1_699_999_500_000),
            retrieved_at_ms: 1_700_000_000_000,
            content: RetainedContent::RetainedBlob {
                content_hash: shared_hash.clone(),
                byte_len: 28,
                media_type: "text/plain".to_owned(),
            },
        };
        let rec_b = SourceRecord {
            envelope: ObjectEnvelope {
                id: OpaqueId::new("sr_mirror_b")?,
                version_id: VersionId::new("ver_sr_mirror_b_1")?,
                ..rec_a.envelope.clone()
            },
            source_id: OpaqueId::new("src_publisher_b")?,
            acquisition_attempt_id: OpaqueId::new("acq_2")?,
            source_locator: "https://publisher-b.example/reprint/1".to_owned(),
            ..rec_a.clone()
        };
        assert_ne!(rec_a.envelope.id, rec_b.envelope.id);
        assert_ne!(rec_a.envelope.version_id, rec_b.envelope.version_id);
        Ok(())
    }

    /// Verifies that non-observational classes (inferences, predictive derivatives) cannot be
    /// promoted to observations, missing values are preserved, and source corrections flag
    /// downstream dependents without mutating issued versions (#10, A01, A07).
    #[test]
    fn origin_promotion_missing_values_and_reassessment_walk() -> Result<(), DomainError> {
        // 1. Inference or predictive derivative cannot become NormalizedObservation.
        let bad_promo = OriginClass::validate_derivation(
            &[
                OriginClass::NormalizedObservation,
                OriginClass::AnalyticalInference,
            ],
            OriginClass::NormalizedObservation,
        );
        assert_eq!(
            bad_promo,
            Err(DomainError::IllegalOriginPromotion {
                target: OriginClass::NormalizedObservation
            })
        );
        assert!(
            OriginClass::validate_derivation(
                &[
                    OriginClass::NormalizedObservation,
                    OriginClass::AnalyticalInference
                ],
                OriginClass::AnalyticalInference,
            )
            .is_ok()
        );

        // 2. Non-finite floats and empty missing reasons are rejected; explicit Missing is preserved.
        let nan_obs = RecordedValue::Measured {
            value: f64::NAN,
            unit: "m".to_owned(),
            uncertainty: None,
        };
        assert_eq!(
            nan_obs.validate(),
            Err(DomainError::NonFiniteNumericValue("observation.value"))
        );
        let missing_alt = RecordedValue::Missing {
            reason: "transponder altitude field absent".to_owned(),
        };
        missing_alt.validate()?;

        // 3. Dependency walk marks downstream claim and report NeedsReassessment when source is corrected,
        //    and independent_source_roots collapses syndicated duplicates and citation cycles.
        let mut graph = EvidenceGraph::new();
        let actor = ActorId::new("analyst_1")?;
        let case_id = OpaqueId::new("case_phoenix")?;

        let make_env = |id: &str,
                        ver: &str,
                        obj_type: DomainObjectType,
                        src_vers: Vec<VersionId>|
         -> Result<ObjectEnvelope, DomainError> {
            Ok(ObjectEnvelope {
                id: OpaqueId::new(id)?,
                version_id: VersionId::new(ver)?,
                schema_version: SchemaVersion::V1,
                object_type: obj_type,
                created_at_ms: 1_700_000_000_000,
                created_by: actor.clone(),
                visibility: WorkspaceVisibility::PrivateCase {
                    case_id: case_id.clone(),
                    sensitive: false,
                },
                supersedes: None,
                provenance: ProvenanceRecord {
                    declared_origin_note: if src_vers.is_empty() {
                        Some("Root source acquisition".to_owned())
                    } else {
                        None
                    },
                    source_version_ids: src_vers,
                    processing_history: Vec::new(),
                },
                origin_class: OriginClass::AcquiredSourceMaterial,
                review_status: ReviewStatus::Reviewed,
                availability: AvailabilityState::Available,
                reassessment: ReassessmentState::Current,
            })
        };

        let src_v1 = VersionId::new("ver_doc_v1")?;
        let doc_v1 = DomainObject::SourceRecord(SourceRecord {
            envelope: make_env(
                "doc_1",
                "ver_doc_v1",
                DomainObjectType::SourceRecord,
                Vec::new(),
            )?,
            source_id: OpaqueId::new("src_archive")?,
            upstream_version: "v1".to_owned(),
            acquisition_attempt_id: OpaqueId::new("acq_1")?,
            source_locator: "https://archive.example/doc1".to_owned(),
            source_event_time_ms: None,
            public_availability_time_ms: Some(1_690_000_000_000),
            retrieved_at_ms: 1_700_000_000_000,
            content: RetainedContent::Omitted {
                reason: OmissionReason::UpstreamReferenceOnly,
                known_upstream_hash: None,
            },
        });
        graph.insert_object(doc_v1, None)?;

        let claim_v1 = VersionId::new("ver_claim_1")?;
        let claim_obj = DomainObject::Claim(Claim {
            envelope: ObjectEnvelope {
                origin_class: OriginClass::ExtractedClaim,
                ..make_env(
                    "claim_1",
                    "ver_claim_1",
                    DomainObjectType::Claim,
                    vec![src_v1.clone()],
                )?
            },
            proposition: "Object maintained stationary hover for 180 seconds".to_owned(),
            claimant: "Witness A".to_owned(),
            nature: ClaimNature::AttributedStatement {
                speaker: "Witness A".to_owned(),
            },
            supporting_anchor: None,
            assessment: ClaimAssessment::Unresolved,
            assessment_rationale: "Single uncorroborated witness statement".to_owned(),
        });
        graph.insert_object(claim_obj, None)?;

        let report_v1 = VersionId::new("ver_report_1")?;
        let report_obj = DomainObject::Report(Report {
            envelope: ObjectEnvelope {
                origin_class: OriginClass::AnalyticalInference,
                ..make_env(
                    "report_1",
                    "ver_report_1",
                    DomainObjectType::Report,
                    vec![claim_v1.clone()],
                )?
            },
            investigation_id: case_id.clone(),
            title: "Initial Case Summary".to_owned(),
            blocks: Vec::new(),
            limitations_and_open_questions: vec!["Single source".to_owned()],
            publication_state: ReportPublicationState::PrivateDraft,
        });
        graph.insert_object(report_obj, None)?;

        // Supersede doc_1 with a corrected v2; verify claim_v1 and report_v1 are flagged NeedsReassessment
        // while their original propositions/titles remain untouched.
        let mut env_v2 = make_env(
            "doc_1",
            "ver_doc_v2",
            DomainObjectType::SourceRecord,
            Vec::new(),
        )?;
        env_v2.supersedes = Some(src_v1.clone());
        env_v2.created_at_ms = 1_700_000_500_000;
        let doc_v2 = DomainObject::SourceRecord(SourceRecord {
            envelope: env_v2,
            source_id: OpaqueId::new("src_archive")?,
            upstream_version: "v2-corrected".to_owned(),
            acquisition_attempt_id: OpaqueId::new("acq_2")?,
            source_locator: "https://archive.example/doc1".to_owned(),
            source_event_time_ms: None,
            public_availability_time_ms: Some(1_700_000_400_000),
            retrieved_at_ms: 1_700_000_500_000,
            content: RetainedContent::Omitted {
                reason: OmissionReason::UpstreamReferenceOnly,
                known_upstream_hash: None,
            },
        });
        graph.insert_object(doc_v2, Some("Source timing corrected by archive erratum"))?;

        let stored_claim = graph
            .get_version(&claim_v1)
            .ok_or(DomainError::MissingProvenance)?;
        assert!(matches!(
            stored_claim.envelope().reassessment,
            ReassessmentState::NeedsReassessment { .. }
        ));
        let stored_report = graph
            .get_version(&report_v1)
            .ok_or(DomainError::MissingProvenance)?;
        assert!(matches!(
            stored_report.envelope().reassessment,
            ReassessmentState::NeedsReassessment { .. }
        ));

        // Verify derivation cycle rejection vs lineage root deduplication.
        let cycle_err = graph.add_edge(RelationEdge {
            edge_id: OpaqueId::new("edge_self")?,
            from_version: claim_v1.clone(),
            to_version: claim_v1.clone(),
            kind: RelationKind::DerivedFrom,
            rationale: "invalid self cycle".to_owned(),
            review_status: ReviewStatus::Unreviewed,
        });
        assert!(matches!(
            cycle_err,
            Err(DomainError::CyclicDerivation { .. })
        ));

        let copy_a = VersionId::new("ver_syndicated_a")?;
        let copy_b = VersionId::new("ver_syndicated_b")?;
        graph.add_edge(RelationEdge {
            edge_id: OpaqueId::new("repub_a")?,
            from_version: copy_a.clone(),
            to_version: src_v1.clone(),
            kind: RelationKind::Republishes,
            rationale: "Wire reprint A".to_owned(),
            review_status: ReviewStatus::Reviewed,
        })?;
        graph.add_edge(RelationEdge {
            edge_id: OpaqueId::new("repub_b")?,
            from_version: copy_b.clone(),
            to_version: copy_a.clone(),
            kind: RelationKind::Quotes,
            rationale: "Quotes reprint A".to_owned(),
            review_status: ReviewStatus::Reviewed,
        })?;
        let roots = graph.independent_source_roots(&[copy_a, copy_b]);
        assert_eq!(roots.len(), 1);
        assert!(roots.contains(&src_v1));

        Ok(())
    }
}
