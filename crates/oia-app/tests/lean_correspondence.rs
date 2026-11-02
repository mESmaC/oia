//! Full workspace Rust-to-Lean correspondence fixture suite (`#15`, ADR 0005).
//!
//! Verifies that `oia_time_geo::select_frame_in_window`, `oia_forecast::is_admissible_at_cutoff`,
//! `oia_analysis::compute_invalidated_nodes`, and `oia_domain::EvidenceGraph::propagate_reassessment`
//! produce identical outputs to the `#guard` assertions in `proofs/OiaProofs/Fixtures.lean`.

use oia_analysis::compute_invalidated_nodes;
use oia_forecast::{CutoffMode, is_admissible_at_cutoff};
use oia_time_geo::{FrameSelectionOutcome, select_frame_in_window};

#[test]
fn lean_correspondence_all_targets_match_fixtures() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        select_frame_in_window(&[], 1_000, 5_000, 3_000, 600)?,
        FrameSelectionOutcome::NoFramesAvailable
    );
    assert_eq!(
        select_frame_in_window(
            &[2_400, 2_950, 2_700, 3_000, 3_001],
            1_000,
            5_000,
            3_000,
            600
        )?,
        FrameSelectionOutcome::Selected(3_000)
    );

    assert!(!is_admissible_at_cutoff(
        CutoffMode::StrictProspective,
        1_000,
        None,
        500
    ));
    assert!(is_admissible_at_cutoff(
        CutoffMode::StrictProspective,
        1_000,
        Some(900),
        1_000
    ));
    assert!(!is_admissible_at_cutoff(
        CutoffMode::StrictProspective,
        1_000,
        Some(900),
        1_001
    ));
    assert!(is_admissible_at_cutoff(
        CutoffMode::RetrospectiveReconstruction,
        1_000,
        Some(900),
        1_500
    ));
    assert!(!is_admissible_at_cutoff(
        CutoffMode::RetrospectiveReconstruction,
        1_000,
        Some(1_001),
        500
    ));

    let edges = [(2, 1), (3, 2), (4, 1)];
    assert_eq!(compute_invalidated_nodes(&edges, 1), vec![1, 2, 3, 4]);

    // Also verify `EvidenceGraph::propagate_reassessment` directly on the identical 4-node fixture:
    let mut graph = oia_domain::EvidenceGraph::new();
    let actor = oia_domain::ActorId::new("actor-1")?;
    for (idx, kind, src_vers) in [
        (1, oia_domain::OriginClass::AcquiredSourceMaterial, vec![]),
        (
            2,
            oia_domain::OriginClass::ExtractedClaim,
            vec![oia_domain::VersionId::new("v-1")?],
        ),
        (
            3,
            oia_domain::OriginClass::AnalyticalInference,
            vec![oia_domain::VersionId::new("v-2")?],
        ),
        (
            4,
            oia_domain::OriginClass::ExtractedClaim,
            vec![oia_domain::VersionId::new("v-1")?],
        ),
    ] {
        graph.insert_object(
            oia_domain::DomainObject::Claim(oia_domain::Claim {
                envelope: oia_domain::ObjectEnvelope {
                    id: oia_domain::OpaqueId::new(format!("node-{idx}"))?,
                    version_id: oia_domain::VersionId::new(format!("v-{idx}"))?,
                    schema_version: oia_domain::SchemaVersion::V1,
                    object_type: oia_domain::DomainObjectType::Claim,
                    created_at_ms: 1_000,
                    created_by: actor.clone(),
                    visibility: oia_domain::WorkspaceVisibility::LibraryPublic,
                    supersedes: None,
                    provenance: oia_domain::ProvenanceRecord {
                        source_version_ids: src_vers,
                        declared_origin_note: if idx == 1 {
                            Some("root fixture node".to_owned())
                        } else {
                            None
                        },
                        processing_history: Vec::new(),
                    },
                    origin_class: kind,
                    review_status: oia_domain::ReviewStatus::Reviewed,
                    availability: oia_domain::AvailabilityState::Available,
                    reassessment: oia_domain::ReassessmentState::Current,
                },
                proposition: format!("Node {idx}"),
                claimant: "Fixture".to_owned(),
                nature: oia_domain::ClaimNature::AttributedStatement {
                    speaker: "Fixture".to_owned(),
                },
                supporting_anchor: None,
                assessment: oia_domain::ClaimAssessment::Supported,
                assessment_rationale: "Fixture node".to_owned(),
            }),
            None,
        )?;
    }
    let mut flagged = graph.propagate_reassessment(
        &oia_domain::VersionId::new("v-1")?,
        "Upstream root node-1 revised",
        2_000,
    );
    flagged.sort();
    assert_eq!(
        flagged,
        vec![
            oia_domain::VersionId::new("v-2")?,
            oia_domain::VersionId::new("v-3")?,
            oia_domain::VersionId::new("v-4")?,
        ]
    );

    Ok(())
}
