//! Contract tests for `oia_analysis::research_links` (`INTEL-02`, `INTEL-04`,
//! `REASON-03`, `A08` subset, `#36`).
//!
//! Risk and requirement coverage:
//! 1. `a08_public_links_conflicting_identity_circular_citations_and_maturity_contract`:
//!    Verifies that conflicting identity matches remain `DisputedIdentity` until explicitly
//!    reversed with a rationale (`REASON-03`, `A08`), `ObservedCoOccurrence` stays separate
//!    from substantive funding/contract links (`docs/architecture.md` §2.3), circular
//!    `CitesPublication` chains (and publications citing exclusively into a cycle) are
//!    reported as structure while leaving corroboration verdicts to `oia_documents::lineage`
//!    (`AGENTS.md` §4 Option (b)), patents/grants/proposals and simulated/non-observational
//!    sources can never establish demonstrated capability (`INTEL-02`, `AGENTS.md` §2 & §3),
//!    item-declared funding is kept separate from link-confirmed funding, and out-of-set
//!    lineage plus source corrections/retractions are preserved.
//! 2. `rejects_invalid_endpoints_format_controls_and_preserves_scc_soundness_on_cycle_truncation`:
//!    Verifies fail-closed rejection of self-citations, invalid endpoint kinds, invisible
//!    Unicode format/bidi controls (while permitting ZWNJ/ZWJ in multilingual names), and
//!    cooperative cancellation (`StopReason::Cancelled`), and verifies that when elementary
//!    cycle enumeration hits `max_reported_cycles`, iterative Tarjan SCC cycle membership
//!    remains exact and `CitationCycleSearchTruncated` is reported.
//! 3. `excludes_non_observational_origins_and_retracted_sources_and_propagates_upstream_reassessment`:
//!    Verifies that `IllustrativeRecreation`, `Forecast`, `AnalyticalInference`, and
//!    `PredictiveDerivative` sources (as well as `Retracted` sources) never count as reported
//!    demonstrations or confirmed links, and that upstream `CorrectedBy`/`Retracted` states
//!    propagate transitively through `derived_from_versions`.

use oia_analysis::research_links::{
    AlternateName, AlternateNameKind, AssessedTechnicalStage, CIRCULAR_CITATION_CAVEAT,
    CO_OCCURRENCE_CAVEAT, ClaimedTechnicalStage, DecisionState, DemonstrationCaveat,
    DemonstrationScope, EvidenceOriginClass, IDENTITY_RESOLUTION_CAVEAT, IdentityCandidateMatch,
    IdentityMention, LinkEffectiveStatus, LinkEndpoint, METHOD_ID, MentionResolution,
    OverallTechnicalMaturity, PublicEntityKind, PublicLinkRecord, PublicLinkRelation,
    ReassessmentReason, ResearchEntity, ResearchLinkError, ResearchLinkLimitKind,
    ResearchLinkLimitation, ResearchLinkLimits, ResearchLinkRequest, ResearchSourceRecord,
    SIMULATED_SOURCE_CAVEAT, SourceLifecycleState, StopReason, TECHNICAL_MATURITY_CAVEAT,
    TechnicalArtifactKind, TechnicalDevelopmentInput, TechnicalEvidenceItem,
    evaluate_research_links,
};
use oia_domain::{ActorId, IdError, OpaqueId, VersionId};

#[derive(Debug)]
enum TestError {
    Id(IdError),
    Kernel(ResearchLinkError),
}

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Id(err) => write!(f, "{err}"),
            Self::Kernel(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for TestError {}

impl From<IdError> for TestError {
    fn from(err: IdError) -> Self {
        Self::Id(err)
    }
}

impl From<ResearchLinkError> for TestError {
    fn from(err: ResearchLinkError) -> Self {
        Self::Kernel(err)
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn a08_public_links_conflicting_identity_circular_citations_and_maturity_contract()
-> Result<(), TestError> {
    let analyst = ActorId::new("actor:reviewer-a08")?;

    let v_ledger = VersionId::new("ver:grant-ledger-v1")?;
    let v_patent = VersionId::new("ver:patent-spec-v1")?;
    let v_patent_v2 = VersionId::new("ver:patent-spec-v2")?;
    let v_pub_a = VersionId::new("ver:pub-alpha-v1")?;
    let v_pub_b = VersionId::new("ver:pub-beta-v1")?;
    let v_pub_c = VersionId::new("ver:pub-gamma-v1")?;
    let v_lab_root = VersionId::new("ver:lab-tr-root-v1")?;
    let v_lab_derived = VersionId::new("ver:lab-tr-derived-v1")?;
    let v_field_indep = VersionId::new("ver:field-tr-indep-v1")?;
    let v_sim_root = VersionId::new("ver:sim-fixture-v1")?;
    let v_sim_derived = VersionId::new("ver:sim-derived-v1")?;
    let v_external_unloaded = VersionId::new("ver:external-archive-v0")?;

    let sources = vec![
        ResearchSourceRecord {
            source_id: OpaqueId::new("src:grant-ledger")?,
            version_id: v_ledger.clone(),
            origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
            lifecycle: SourceLifecycleState::Current,
            derived_from_versions: vec![v_external_unloaded.clone()],
            citation_label: "Synthetic Grant Ledger".to_owned(),
        },
        ResearchSourceRecord {
            source_id: OpaqueId::new("src:patent-spec")?,
            version_id: v_patent.clone(),
            origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
            lifecycle: SourceLifecycleState::CorrectedBy {
                replacement_version: v_patent_v2.clone(),
                summary: "Corrected assignee city in v2".to_owned(),
            },
            derived_from_versions: Vec::new(),
            citation_label: "Synthetic Patent US-2026-0999100-A1".to_owned(),
        },
        ResearchSourceRecord {
            source_id: OpaqueId::new("src:pub-alpha")?,
            version_id: v_pub_a.clone(),
            origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
            lifecycle: SourceLifecycleState::Current,
            derived_from_versions: Vec::new(),
            citation_label: "Preprint Alpha".to_owned(),
        },
        ResearchSourceRecord {
            source_id: OpaqueId::new("src:pub-beta")?,
            version_id: v_pub_b.clone(),
            origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
            lifecycle: SourceLifecycleState::Current,
            derived_from_versions: Vec::new(),
            citation_label: "Survey Beta".to_owned(),
        },
        ResearchSourceRecord {
            source_id: OpaqueId::new("src:pub-gamma")?,
            version_id: v_pub_c.clone(),
            origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
            lifecycle: SourceLifecycleState::Current,
            derived_from_versions: Vec::new(),
            citation_label: "Commentary Gamma citing Alpha".to_owned(),
        },
        ResearchSourceRecord {
            source_id: OpaqueId::new("src:lab-tr-root")?,
            version_id: v_lab_root.clone(),
            origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
            lifecycle: SourceLifecycleState::Current,
            derived_from_versions: Vec::new(),
            citation_label: "Chamber B Vacuum Test Report".to_owned(),
        },
        ResearchSourceRecord {
            source_id: OpaqueId::new("src:lab-tr-derived")?,
            version_id: v_lab_derived.clone(),
            origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
            lifecycle: SourceLifecycleState::Current,
            derived_from_versions: vec![v_lab_root.clone()],
            citation_label: "Conference Summary of Chamber B Vacuum Test".to_owned(),
        },
        ResearchSourceRecord {
            source_id: OpaqueId::new("src:field-tr-indep")?,
            version_id: v_field_indep.clone(),
            origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
            lifecycle: SourceLifecycleState::Current,
            derived_from_versions: Vec::new(),
            citation_label: "Independent Open-Range Telemetry Report".to_owned(),
        },
        ResearchSourceRecord {
            source_id: OpaqueId::new("src:sim-fixture")?,
            version_id: v_sim_root.clone(),
            origin_class: EvidenceOriginClass::Simulation,
            lifecycle: SourceLifecycleState::Current,
            derived_from_versions: Vec::new(),
            citation_label: "Synthetic Simulation Fixture".to_owned(),
        },
        ResearchSourceRecord {
            source_id: OpaqueId::new("src:sim-derived")?,
            version_id: v_sim_derived.clone(),
            origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
            lifecycle: SourceLifecycleState::Current,
            derived_from_versions: vec![v_sim_root.clone()],
            citation_label: "Report Derived Solely From Simulation".to_owned(),
        },
    ];

    let org_sponsor = OpaqueId::new("ent:org-sponsor")?;
    let org_north = OpaqueId::new("ent:org-aerolab-north")?;
    let org_south = OpaqueId::new("ent:org-aerolab-south")?;
    let grant_404 = OpaqueId::new("ent:grant-404")?;
    let contract_808 = OpaqueId::new("ent:contract-808")?;
    let fac_chamber = OpaqueId::new("ent:fac-chamber-b")?;
    let pub_alpha = OpaqueId::new("ent:pub-alpha")?;
    let pub_beta = OpaqueId::new("ent:pub-beta")?;
    let pub_gamma = OpaqueId::new("ent:pub-gamma")?;
    let pub_lab_root = OpaqueId::new("ent:pub-lab-root")?;
    let pub_lab_derived = OpaqueId::new("ent:pub-lab-derived")?;
    let pub_field_indep = OpaqueId::new("ent:pub-field-indep")?;
    let evt_symposium = OpaqueId::new("ent:evt-symposium")?;

    let entities = vec![
        ResearchEntity {
            entity_id: org_sponsor.clone(),
            kind: PublicEntityKind::Organization,
            primary_name: "Civic Science Foundation".to_owned(),
            alternate_names: vec![AlternateName {
                name: "CSF".to_owned(),
                kind: AlternateNameKind::Acronym,
                source_version: v_ledger.clone(),
            }],
            defining_source_version: v_ledger.clone(),
        },
        ResearchEntity {
            entity_id: org_north.clone(),
            kind: PublicEntityKind::Organization,
            primary_name: "Aerospace Propulsion Laboratory North".to_owned(),
            alternate_names: vec![
                AlternateName {
                    name: "AERO-LAB".to_owned(),
                    kind: AlternateNameKind::Acronym,
                    source_version: v_ledger.clone(),
                },
                AlternateName {
                    name: "АэроЛаб-Север".to_owned(),
                    kind: AlternateNameKind::Transliteration {
                        script: "Cyrl".to_owned(),
                    },
                    source_version: v_ledger.clone(),
                },
                // ZWNJ (U+200C) in Persian alternate name is preserved (`INTEL-04`).
                AlternateName {
                    name: "پژوهشگاه\u{200C}هوافضا".to_owned(),
                    kind: AlternateNameKind::TranslatedTitle {
                        language: "fa".to_owned(),
                    },
                    source_version: v_ledger.clone(),
                },
            ],
            defining_source_version: v_ledger.clone(),
        },
        ResearchEntity {
            entity_id: org_south.clone(),
            kind: PublicEntityKind::Organization,
            primary_name: "Aero-Lab Diagnostics South LLC".to_owned(),
            alternate_names: vec![AlternateName {
                name: "AERO-LAB".to_owned(),
                kind: AlternateNameKind::FormerOrTradeName,
                source_version: v_ledger.clone(),
            }],
            defining_source_version: v_ledger.clone(),
        },
        ResearchEntity {
            entity_id: grant_404.clone(),
            kind: PublicEntityKind::Grant,
            primary_name: "Grant CSF-404".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: v_ledger.clone(),
        },
        ResearchEntity {
            entity_id: contract_808.clone(),
            kind: PublicEntityKind::Contract,
            primary_name: "Contract CTA-808".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: v_ledger.clone(),
        },
        ResearchEntity {
            entity_id: fac_chamber,
            kind: PublicEntityKind::Facility,
            primary_name: "Chamber B".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: v_lab_root.clone(),
        },
        ResearchEntity {
            entity_id: pub_alpha.clone(),
            kind: PublicEntityKind::Publication,
            primary_name: "Preprint Alpha".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: v_pub_a.clone(),
        },
        ResearchEntity {
            entity_id: pub_beta.clone(),
            kind: PublicEntityKind::Publication,
            primary_name: "Survey Beta".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: v_pub_b.clone(),
        },
        ResearchEntity {
            entity_id: pub_gamma.clone(),
            kind: PublicEntityKind::Publication,
            primary_name: "Commentary Gamma".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: v_pub_c.clone(),
        },
        ResearchEntity {
            entity_id: pub_lab_root.clone(),
            kind: PublicEntityKind::Publication,
            primary_name: "Chamber B Test Report".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: v_lab_root.clone(),
        },
        ResearchEntity {
            entity_id: pub_lab_derived.clone(),
            kind: PublicEntityKind::Publication,
            primary_name: "Chamber B Summary".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: v_lab_derived.clone(),
        },
        ResearchEntity {
            entity_id: pub_field_indep.clone(),
            kind: PublicEntityKind::Publication,
            primary_name: "Independent Range Telemetry Report".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: v_field_indep.clone(),
        },
        ResearchEntity {
            entity_id: evt_symposium.clone(),
            kind: PublicEntityKind::Event,
            primary_name: "Propulsion Symposium".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: v_ledger.clone(),
        },
    ];

    let mention_awardee = OpaqueId::new("mention:awardee-aerolab")?;
    let mention_unlinked = OpaqueId::new("mention:reversed-contractor")?;

    let mut request = ResearchLinkRequest {
        limits: ResearchLinkLimits::default(),
        sources,
        entities,
        mentions: vec![
            IdentityMention {
                mention_id: mention_awardee.clone(),
                verbatim_text: "AERO-LAB".to_owned(),
                source_version: v_ledger.clone(),
                expected_kind: PublicEntityKind::Organization,
                candidates: vec![
                    IdentityCandidateMatch {
                        candidate_id: OpaqueId::new("cand:north")?,
                        target_entity_id: org_north.clone(),
                        matching_basis: "Shared acronym AERO-LAB and facility registry".to_owned(),
                        supporting_source_version: v_ledger.clone(),
                        decision: DecisionState::Confirmed {
                            actor: analyst.clone(),
                            rationale: "Confirmed UEI match".to_owned(),
                        },
                    },
                    IdentityCandidateMatch {
                        candidate_id: OpaqueId::new("cand:south")?,
                        target_entity_id: org_south.clone(),
                        matching_basis: "Shared trade acronym AERO-LAB".to_owned(),
                        supporting_source_version: v_ledger.clone(),
                        decision: DecisionState::Proposed {
                            rationale: "Initial lexical match on AERO-LAB".to_owned(),
                        },
                    },
                ],
            },
            IdentityMention {
                mention_id: mention_unlinked.clone(),
                verbatim_text: "Former Subcontractor X".to_owned(),
                source_version: v_ledger.clone(),
                expected_kind: PublicEntityKind::Organization,
                candidates: vec![IdentityCandidateMatch {
                    candidate_id: OpaqueId::new("cand:unlinked-south")?,
                    target_entity_id: org_south.clone(),
                    matching_basis: "Stale directory listing".to_owned(),
                    supporting_source_version: v_ledger.clone(),
                    decision: DecisionState::Reversed {
                        actor: analyst.clone(),
                        reversal_rationale: "Directory listing belonged to an unrelated state"
                            .to_owned(),
                    },
                }],
            },
        ],
        links: vec![
            PublicLinkRecord {
                link_id: OpaqueId::new("link:sponsor-to-grant")?,
                from_endpoint: LinkEndpoint::Entity(org_sponsor),
                to_endpoint: LinkEndpoint::Entity(grant_404.clone()),
                relation: PublicLinkRelation::GrantsFundingTo,
                source_version: v_ledger.clone(),
                verbatim_excerpt: "CSF funded Grant 404".to_owned(),
                decision: DecisionState::Confirmed {
                    actor: analyst.clone(),
                    rationale: "Award table".to_owned(),
                },
            },
            PublicLinkRecord {
                link_id: OpaqueId::new("link:grant-to-awardee-mention")?,
                from_endpoint: LinkEndpoint::Entity(grant_404.clone()),
                to_endpoint: LinkEndpoint::Mention(mention_awardee.clone()),
                relation: PublicLinkRelation::GrantsFundingTo,
                source_version: v_ledger.clone(),
                verbatim_excerpt: "Grant 404 recipient: AERO-LAB".to_owned(),
                decision: DecisionState::Confirmed {
                    actor: analyst.clone(),
                    rationale: "Recipient column".to_owned(),
                },
            },
            PublicLinkRecord {
                link_id: OpaqueId::new("link:contract-to-reversed-mention")?,
                from_endpoint: LinkEndpoint::Entity(contract_808.clone()),
                to_endpoint: LinkEndpoint::Mention(mention_unlinked),
                relation: PublicLinkRelation::AwardsContractTo,
                source_version: v_ledger.clone(),
                verbatim_excerpt: "Contract 808 sub-recipient".to_owned(),
                decision: DecisionState::Confirmed {
                    actor: analyst.clone(),
                    rationale: "Initial import".to_owned(),
                },
            },
            PublicLinkRecord {
                link_id: OpaqueId::new("link:co-occurrence-only")?,
                from_endpoint: LinkEndpoint::Entity(org_south.clone()),
                to_endpoint: LinkEndpoint::Entity(evt_symposium),
                relation: PublicLinkRelation::ObservedCoOccurrence,
                source_version: v_ledger,
                verbatim_excerpt: "Both listed on symposium flyer".to_owned(),
                decision: DecisionState::Confirmed {
                    actor: analyst.clone(),
                    rationale: "Co-occurrence only".to_owned(),
                },
            },
            PublicLinkRecord {
                link_id: OpaqueId::new("link:cite-a-b")?,
                from_endpoint: LinkEndpoint::Entity(pub_alpha.clone()),
                to_endpoint: LinkEndpoint::Entity(pub_beta.clone()),
                relation: PublicLinkRelation::CitesPublication,
                source_version: v_pub_a,
                verbatim_excerpt: "Alpha cites Beta".to_owned(),
                decision: DecisionState::Confirmed {
                    actor: analyst.clone(),
                    rationale: "Ref 1".to_owned(),
                },
            },
            PublicLinkRecord {
                link_id: OpaqueId::new("link:cite-b-a")?,
                from_endpoint: LinkEndpoint::Entity(pub_beta.clone()),
                to_endpoint: LinkEndpoint::Entity(pub_alpha.clone()),
                relation: PublicLinkRelation::CitesPublication,
                source_version: v_pub_b,
                verbatim_excerpt: "Beta cites Alpha".to_owned(),
                decision: DecisionState::Confirmed {
                    actor: analyst.clone(),
                    rationale: "Ref 2".to_owned(),
                },
            },
            PublicLinkRecord {
                link_id: OpaqueId::new("link:cite-c-a")?,
                from_endpoint: LinkEndpoint::Entity(pub_gamma.clone()),
                to_endpoint: LinkEndpoint::Entity(pub_alpha.clone()),
                relation: PublicLinkRelation::CitesPublication,
                source_version: v_pub_c.clone(),
                verbatim_excerpt: "Gamma cites only Alpha".to_owned(),
                decision: DecisionState::Confirmed {
                    actor: analyst.clone(),
                    rationale: "Ref 3".to_owned(),
                },
            },
        ],
        developments: vec![
            TechnicalDevelopmentInput {
                development_id: OpaqueId::new("dev:patent-and-circular")?,
                topic: "Claimed field flight assembly".to_owned(),
                subject_endpoint: LinkEndpoint::Mention(mention_awardee.clone()),
                items: vec![
                    TechnicalEvidenceItem {
                        item_id: OpaqueId::new("item:patent-overclaim")?,
                        artifact_kind: TechnicalArtifactKind::PatentApplicationOrGrant,
                        claimed_stage: ClaimedTechnicalStage::FieldOrOperationalDemonstration,
                        publication_entity_id: None,
                        funding_or_contract_ids: Vec::new(),
                        source_version: v_patent.clone(),
                        verbatim_claim: "Patent claims operational flight".to_owned(),
                        stated_conditions_and_limitations: "Patent specification only".to_owned(),
                        decision: DecisionState::Confirmed {
                            actor: analyst.clone(),
                            rationale: "Inspected patent text".to_owned(),
                        },
                    },
                    TechnicalEvidenceItem {
                        item_id: OpaqueId::new("item:gamma-circular-demo")?,
                        artifact_kind: TechnicalArtifactKind::LaboratoryTestReport,
                        claimed_stage: ClaimedTechnicalStage::LaboratoryDemonstration,
                        publication_entity_id: Some(pub_gamma.clone()),
                        funding_or_contract_ids: Vec::new(),
                        source_version: v_pub_c,
                        verbatim_claim: "Gamma repeats Alpha/Beta loop".to_owned(),
                        stated_conditions_and_limitations: "No independent test stand".to_owned(),
                        decision: DecisionState::Confirmed {
                            actor: analyst.clone(),
                            rationale: "Cites into Alpha-Beta cycle".to_owned(),
                        },
                    },
                    TechnicalEvidenceItem {
                        item_id: OpaqueId::new("item:sim-derived-demo")?,
                        artifact_kind: TechnicalArtifactKind::FieldOrOperationalTestReport,
                        claimed_stage: ClaimedTechnicalStage::FieldOrOperationalDemonstration,
                        publication_entity_id: None,
                        funding_or_contract_ids: Vec::new(),
                        source_version: v_sim_derived.clone(),
                        verbatim_claim: "Derived from simulation fixture".to_owned(),
                        stated_conditions_and_limitations: "Upstream is simulation".to_owned(),
                        decision: DecisionState::Confirmed {
                            actor: analyst.clone(),
                            rationale: "Lineage check".to_owned(),
                        },
                    },
                ],
            },
            TechnicalDevelopmentInput {
                development_id: OpaqueId::new("dev:instrumented-thruster")?,
                topic: "Instrumented thruster testing".to_owned(),
                subject_endpoint: LinkEndpoint::Mention(mention_awardee.clone()),
                items: vec![
                    TechnicalEvidenceItem {
                        item_id: OpaqueId::new("item:lab-root")?,
                        artifact_kind: TechnicalArtifactKind::LaboratoryTestReport,
                        claimed_stage: ClaimedTechnicalStage::FieldOrOperationalDemonstration,
                        publication_entity_id: Some(pub_lab_root),
                        funding_or_contract_ids: Vec::new(),
                        source_version: v_lab_root.clone(),
                        verbatim_claim: "42 mN in vacuum chamber".to_owned(),
                        stated_conditions_and_limitations: "Vacuum chamber only".to_owned(),
                        decision: DecisionState::Confirmed {
                            actor: analyst.clone(),
                            rationale: "Lab report".to_owned(),
                        },
                    },
                    TechnicalEvidenceItem {
                        item_id: OpaqueId::new("item:lab-derived")?,
                        artifact_kind: TechnicalArtifactKind::LaboratoryTestReport,
                        claimed_stage: ClaimedTechnicalStage::LaboratoryDemonstration,
                        publication_entity_id: Some(pub_lab_derived),
                        funding_or_contract_ids: Vec::new(),
                        source_version: v_lab_derived.clone(),
                        verbatim_claim: "Summary of the 42 mN chamber test".to_owned(),
                        stated_conditions_and_limitations: "Same upstream test".to_owned(),
                        decision: DecisionState::Confirmed {
                            actor: analyst.clone(),
                            rationale: "Derived from v_lab_root".to_owned(),
                        },
                    },
                    TechnicalEvidenceItem {
                        item_id: OpaqueId::new("item:field-indep")?,
                        artifact_kind: TechnicalArtifactKind::FieldOrOperationalTestReport,
                        claimed_stage: ClaimedTechnicalStage::FieldOrOperationalDemonstration,
                        publication_entity_id: Some(pub_field_indep),
                        funding_or_contract_ids: vec![contract_808.clone()],
                        source_version: v_field_indep.clone(),
                        verbatim_claim: "Independent open-range test telemetry".to_owned(),
                        stated_conditions_and_limitations: "Suborbital open-air range".to_owned(),
                        decision: DecisionState::Confirmed {
                            actor: analyst.clone(),
                            rationale: "Independent range report".to_owned(),
                        },
                    },
                ],
            },
        ],
    };

    // Pass 1: Candidate `cand:south` is still `Proposed`, so `mention:awardee-aerolab` is DisputedIdentity.
    let report_disputed = evaluate_research_links(&request, || None)?;
    assert_eq!(report_disputed.method_id, METHOD_ID);
    assert_eq!(
        report_disputed.identity_resolution_caveat,
        IDENTITY_RESOLUTION_CAVEAT
    );
    assert_eq!(report_disputed.co_occurrence_caveat, CO_OCCURRENCE_CAVEAT);
    assert_eq!(
        report_disputed.technical_maturity_caveat,
        TECHNICAL_MATURITY_CAVEAT
    );
    assert_eq!(
        report_disputed.circular_citation_caveat,
        CIRCULAR_CITATION_CAVEAT
    );
    assert_eq!(
        report_disputed.simulated_source_caveat,
        SIMULATED_SOURCE_CAVEAT
    );

    // Out-of-set upstream version is retained on the source summary and in limitations.
    assert_eq!(
        report_disputed.sources[0].external_derived_from_versions,
        vec![v_external_unloaded.clone()]
    );
    assert!(
        report_disputed
            .limitations
            .contains(&ResearchLinkLimitation::OutOfSetSourceLineage {
                source_version: VersionId::new("ver:grant-ledger-v1")?,
                external_upstream_versions: vec![v_external_unloaded],
            })
    );

    // Simulation-root lineage: `v_sim_derived` has `AcquiredSourceMaterial` but its in-set
    // ancestor `v_sim_root` is `Simulation`, so `counts_as_reported_public_source` is false
    // and `non_observational_ancestor_versions` names `(v_sim_root, Simulation)`.
    let sim_derived_summary = report_disputed
        .sources
        .iter()
        .find(|s| s.version_id == v_sim_derived)
        .ok_or(ResearchLinkError::EmptySources)?;
    assert!(!sim_derived_summary.counts_as_reported_public_source);
    assert_eq!(
        sim_derived_summary.non_observational_ancestor_versions,
        vec![v_sim_root.clone()]
    );

    // Mention 0 is DisputedIdentity; Mention 1 is AllReversed and retains its reversed candidate history.
    assert_eq!(
        report_disputed.mentions[0].resolution,
        MentionResolution::DisputedIdentity {
            confirmed_entity_ids: vec![org_north.clone()],
            proposed_entity_ids: vec![org_south],
        }
    );
    assert_eq!(
        report_disputed.mentions[1].resolution,
        MentionResolution::AllReversed {
            reversed_candidate_count: 1
        }
    );
    assert_eq!(report_disputed.mentions[1].candidates.len(), 1);
    assert!(matches!(
        report_disputed.mentions[1].candidates[0].decision,
        DecisionState::Reversed { .. }
    ));

    // Link counts: only `link:sponsor-to-grant`, `link:cite-a-b`, `link:cite-b-a`, `link:cite-c-a`
    // are ConfirmedSubstantive (4); `link:grant-to-awardee-mention` is DisputedEndpointIdentity (1);
    // `link:contract-to-reversed-mention` is InactiveReversed; `link:co-occurrence-only` is CoOccurrenceOnly (1).
    assert_eq!(report_disputed.confirmed_substantive_link_count, 4);
    assert_eq!(report_disputed.co_occurrence_link_count, 1);
    assert_eq!(report_disputed.disputed_endpoint_link_count, 1);
    assert_eq!(
        report_disputed.links[1].effective_status,
        LinkEffectiveStatus::DisputedEndpointIdentity {
            disputed_mention_ids: vec![mention_awardee.clone()]
        }
    );
    assert_eq!(
        report_disputed.links[2].effective_status,
        LinkEffectiveStatus::InactiveReversed
    );

    // Citation cycle detection: `pub_alpha -> pub_beta -> pub_alpha` is detected, and `pub_gamma`
    // (which cites only `pub_alpha`) is included in `publications_with_circular_dependency`.
    assert_eq!(report_disputed.citation_cycles.len(), 1);
    assert_eq!(
        report_disputed.citation_cycles[0].publication_chain,
        vec![pub_alpha.clone(), pub_beta.clone(), pub_alpha.clone()]
    );
    assert_eq!(
        report_disputed.publications_with_circular_dependency,
        vec![pub_alpha, pub_beta, pub_gamma.clone()]
    );

    // Development 0 (`dev:patent-and-circular`):
    // - patent item claiming field demo is capped at `PatentSpecificationOnly` with caveat
    //   `ProposalOrPatentCannotEstablishDemonstration`, plus `SourceCorrected` reassessment reason;
    // - gamma lab report is flagged with `CircularCitationChain`;
    // - sim-derived report is capped at `SimulatedEvidenceOnly` with `SimulatedSourceCannotEstablishDemonstration`;
    // - overall maturity is `CircularOrUnconfirmedDemonstrationOnly` with empty `demonstration_source_versions`.
    let dev0 = &report_disputed.developments[0];
    assert!(dev0.subject_identity_disputed);
    assert_eq!(dev0.resolved_subject_entity_id, None);
    assert_eq!(
        dev0.overall_maturity,
        OverallTechnicalMaturity::CircularOrUnconfirmedDemonstrationOnly
    );
    assert!(dev0.demonstration_source_versions.is_empty());
    assert!(dev0.has_unsupported_demonstration_claim);
    assert_eq!(
        dev0.items[0].assessed_stage,
        AssessedTechnicalStage::PatentSpecificationOnly
    );
    assert_eq!(
        dev0.items[0].caveats,
        vec![
            DemonstrationCaveat::ProposalOrPatentCannotEstablishDemonstration {
                artifact_kind: TechnicalArtifactKind::PatentApplicationOrGrant,
                claimed_stage: ClaimedTechnicalStage::FieldOrOperationalDemonstration,
            }
        ]
    );
    assert!(
        dev0.items[0]
            .reassessment_reasons
            .contains(&ReassessmentReason::SourceCorrected {
                source_version: v_patent,
                replacement_version: v_patent_v2,
            })
    );
    assert_eq!(
        dev0.items[1].assessed_stage,
        AssessedTechnicalStage::ConfirmedReportedDemonstration {
            scope: DemonstrationScope::Laboratory,
            in_citation_cycle: true,
        }
    );
    assert_eq!(
        dev0.items[1].caveats,
        vec![DemonstrationCaveat::CircularCitationChain {
            publication_entity_id: pub_gamma,
        }]
    );
    assert_eq!(
        dev0.items[2].assessed_stage,
        AssessedTechnicalStage::SimulatedEvidenceOnly
    );
    assert_eq!(
        dev0.items[2].caveats,
        vec![
            DemonstrationCaveat::SimulatedSourceCannotEstablishDemonstration {
                qualifying_ancestors: vec![(v_sim_root, EvidenceOriginClass::Simulation)],
            }
        ]
    );

    // Development 1 (`dev:instrumented-thruster`):
    // - `item:lab-root` (LaboratoryTestReport claiming FieldOrOperationalDemonstration) is capped
    //   at `DemonstrationScope::Laboratory` with `LaboratoryReportClaimedFieldDemonstration`;
    // - `item:lab-derived` shares lineage root `v_lab_root` with `item:lab-root`, which is reported
    //   structurally in `shared_demonstration_lineage_roots`;
    // - `item_declared_funding_or_contract_ids` contains `contract_808` (whose link was reversed),
    //   while `confirmed_funding_or_contract_ids` is empty while `mention_awardee` is disputed!
    let dev1 = &report_disputed.developments[1];
    assert_eq!(
        dev1.overall_maturity,
        OverallTechnicalMaturity::ReportedDemonstratedCapability {
            highest_scope: DemonstrationScope::FieldOrOperational,
        }
    );
    assert_eq!(
        dev1.demonstration_source_versions,
        vec![v_field_indep, v_lab_derived, v_lab_root.clone()]
    );
    assert_eq!(dev1.shared_demonstration_lineage_roots, vec![v_lab_root]);
    assert_eq!(
        dev1.items[0].assessed_stage,
        AssessedTechnicalStage::ConfirmedReportedDemonstration {
            scope: DemonstrationScope::Laboratory,
            in_citation_cycle: false,
        }
    );
    assert_eq!(
        dev1.items[0].caveats,
        vec![DemonstrationCaveat::LaboratoryReportClaimedFieldDemonstration]
    );
    assert!(dev1.confirmed_funding_or_contract_ids.is_empty());
    assert_eq!(
        dev1.item_declared_funding_or_contract_ids,
        vec![contract_808.clone()]
    );
    assert_eq!(
        dev1.disputed_funding_mention_ids,
        vec![mention_awardee.clone()]
    );

    // Pass 2: Explicitly reverse the conflicting candidate `cand:south` on `mention:awardee-aerolab`.
    request.mentions[0].candidates[1].decision = DecisionState::Reversed {
        actor: analyst,
        reversal_rationale:
            "Confirmed UEI and address belong exclusively to Aerospace Propulsion Laboratory North."
                .to_owned(),
    };
    let report_resolved = evaluate_research_links(&request, || None)?;
    assert_eq!(
        report_resolved.mentions[0].resolution,
        MentionResolution::ConfirmedUnique {
            entity_id: org_north.clone(),
            candidate_id: OpaqueId::new("cand:north")?,
        }
    );
    assert_eq!(report_resolved.disputed_endpoint_link_count, 0);
    assert_eq!(report_resolved.confirmed_substantive_link_count, 5);
    let dev1_resolved = &report_resolved.developments[1];
    assert!(!dev1_resolved.subject_identity_disputed);
    assert_eq!(dev1_resolved.resolved_subject_entity_id, Some(org_north));
    assert_eq!(
        dev1_resolved.confirmed_funding_or_contract_ids,
        vec![grant_404]
    );
    assert_eq!(
        dev1_resolved.item_declared_funding_or_contract_ids,
        vec![contract_808]
    );
    assert!(dev1_resolved.disputed_funding_mention_ids.is_empty());
    assert!(!dev1_resolved.needs_reassessment);

    Ok(())
}

#[test]
#[allow(clippy::too_many_lines)]
fn rejects_invalid_endpoints_format_controls_and_preserves_scc_soundness_on_cycle_truncation()
-> Result<(), TestError> {
    let analyst = ActorId::new("actor:reviewer-a08")?;
    let v1 = VersionId::new("ver:s1")?;
    let pub_a = OpaqueId::new("ent:pub-a")?;
    let pub_b = OpaqueId::new("ent:pub-b")?;
    let pub_c = OpaqueId::new("ent:pub-c")?;
    let fac_x = OpaqueId::new("ent:fac-x")?;

    let base_sources = vec![ResearchSourceRecord {
        source_id: OpaqueId::new("src:s1")?,
        version_id: v1.clone(),
        origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
        lifecycle: SourceLifecycleState::Current,
        derived_from_versions: Vec::new(),
        citation_label: "Synthetic Source 1".to_owned(),
    }];
    let base_entities = vec![
        ResearchEntity {
            entity_id: pub_a.clone(),
            kind: PublicEntityKind::Publication,
            primary_name: "Paper A".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: v1.clone(),
        },
        ResearchEntity {
            entity_id: pub_b.clone(),
            kind: PublicEntityKind::Publication,
            primary_name: "Paper B".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: v1.clone(),
        },
        ResearchEntity {
            entity_id: pub_c.clone(),
            kind: PublicEntityKind::Publication,
            primary_name: "Paper C".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: v1.clone(),
        },
        ResearchEntity {
            entity_id: fac_x.clone(),
            kind: PublicEntityKind::Facility,
            primary_name: "Facility X".to_owned(),
            alternate_names: Vec::new(),
            defining_source_version: v1.clone(),
        },
    ];

    // 1. Self-citation is rejected.
    let self_cite_req = ResearchLinkRequest {
        limits: ResearchLinkLimits::default(),
        sources: base_sources.clone(),
        entities: base_entities.clone(),
        mentions: Vec::new(),
        links: vec![PublicLinkRecord {
            link_id: OpaqueId::new("link:self")?,
            from_endpoint: LinkEndpoint::Entity(pub_a.clone()),
            to_endpoint: LinkEndpoint::Entity(pub_a.clone()),
            relation: PublicLinkRelation::CitesPublication,
            source_version: v1.clone(),
            verbatim_excerpt: "Self citation".to_owned(),
            decision: DecisionState::Confirmed {
                actor: analyst.clone(),
                rationale: "Test".to_owned(),
            },
        }],
        developments: Vec::new(),
    };
    assert_eq!(
        evaluate_research_links(&self_cite_req, || None),
        Err(ResearchLinkError::SelfCitationLink {
            link_id: OpaqueId::new("link:self")?,
            publication_id: pub_a.clone(),
        })
    );

    // 2. Invalid endpoint kinds for `OperatesFacility` (Publication -> Facility) are rejected.
    let bad_kind_req = ResearchLinkRequest {
        limits: ResearchLinkLimits::default(),
        sources: base_sources.clone(),
        entities: base_entities.clone(),
        mentions: Vec::new(),
        links: vec![PublicLinkRecord {
            link_id: OpaqueId::new("link:bad-kind")?,
            from_endpoint: LinkEndpoint::Entity(pub_a.clone()),
            to_endpoint: LinkEndpoint::Entity(fac_x),
            relation: PublicLinkRelation::OperatesFacility,
            source_version: v1.clone(),
            verbatim_excerpt: "Invalid kind".to_owned(),
            decision: DecisionState::Confirmed {
                actor: analyst.clone(),
                rationale: "Test".to_owned(),
            },
        }],
        developments: Vec::new(),
    };
    assert_eq!(
        evaluate_research_links(&bad_kind_req, || None),
        Err(ResearchLinkError::InvalidLinkEndpointKinds {
            link_id: OpaqueId::new("link:bad-kind")?,
            relation: PublicLinkRelation::OperatesFacility,
            from_kind: PublicEntityKind::Publication,
            to_kind: PublicEntityKind::Facility,
        })
    );

    // 3. Format/bidi control characters in rationales are rejected.
    let bidi_req = ResearchLinkRequest {
        limits: ResearchLinkLimits::default(),
        sources: base_sources.clone(),
        entities: base_entities.clone(),
        mentions: Vec::new(),
        links: vec![PublicLinkRecord {
            link_id: OpaqueId::new("link:bidi")?,
            from_endpoint: LinkEndpoint::Entity(pub_a.clone()),
            to_endpoint: LinkEndpoint::Entity(pub_b.clone()),
            relation: PublicLinkRelation::CitesPublication,
            source_version: v1.clone(),
            verbatim_excerpt: "Clean excerpt".to_owned(),
            decision: DecisionState::Reversed {
                actor: analyst.clone(),
                reversal_rationale: "Hidden override \u{202E} inside rationale".to_owned(),
            },
        }],
        developments: Vec::new(),
    };
    assert_eq!(
        evaluate_research_links(&bidi_req, || None),
        Err(ResearchLinkError::ControlOrFormatCharacter {
            context: "link",
            field: "decision.reversal_rationale",
        })
    );

    // 4. Cooperative cancellation returns `Stopped(StopReason::Cancelled)`.
    assert_eq!(
        evaluate_research_links(&self_cite_req, || Some(StopReason::Cancelled)),
        Err(ResearchLinkError::Stopped(StopReason::Cancelled))
    );

    // 5. Elementary cycle truncation (`max_reported_cycles = 1` over a graph with 2 cycles:
    //    `pub_a <-> pub_b` and `pub_b <-> pub_c`) reports `CitationCycleSearchTruncated` while
    //    still marking all 3 publications in `publications_with_circular_dependency` via Tarjan SCCs.
    let tight_limits = ResearchLinkLimits::new(
        16, 16, 8, 16, 8, 16, 16, 16, 8, 8, 256, 4096, 65536, 1, 1024,
    )?;
    assert_eq!(tight_limits.max_reported_cycles(), 1);
    let two_cycle_req = ResearchLinkRequest {
        limits: tight_limits,
        sources: base_sources.clone(),
        entities: base_entities,
        mentions: Vec::new(),
        links: vec![
            PublicLinkRecord {
                link_id: OpaqueId::new("link:ab")?,
                from_endpoint: LinkEndpoint::Entity(pub_a.clone()),
                to_endpoint: LinkEndpoint::Entity(pub_b.clone()),
                relation: PublicLinkRelation::CitesPublication,
                source_version: v1.clone(),
                verbatim_excerpt: "A cites B".to_owned(),
                decision: DecisionState::Confirmed {
                    actor: analyst.clone(),
                    rationale: "r".to_owned(),
                },
            },
            PublicLinkRecord {
                link_id: OpaqueId::new("link:ba")?,
                from_endpoint: LinkEndpoint::Entity(pub_b.clone()),
                to_endpoint: LinkEndpoint::Entity(pub_a.clone()),
                relation: PublicLinkRelation::CitesPublication,
                source_version: v1.clone(),
                verbatim_excerpt: "B cites A".to_owned(),
                decision: DecisionState::Confirmed {
                    actor: analyst.clone(),
                    rationale: "r".to_owned(),
                },
            },
            PublicLinkRecord {
                link_id: OpaqueId::new("link:bc")?,
                from_endpoint: LinkEndpoint::Entity(pub_b.clone()),
                to_endpoint: LinkEndpoint::Entity(pub_c.clone()),
                relation: PublicLinkRelation::CitesPublication,
                source_version: v1.clone(),
                verbatim_excerpt: "B cites C".to_owned(),
                decision: DecisionState::Confirmed {
                    actor: analyst.clone(),
                    rationale: "r".to_owned(),
                },
            },
            PublicLinkRecord {
                link_id: OpaqueId::new("link:cb")?,
                from_endpoint: LinkEndpoint::Entity(pub_c.clone()),
                to_endpoint: LinkEndpoint::Entity(pub_b.clone()),
                relation: PublicLinkRelation::CitesPublication,
                source_version: v1.clone(),
                verbatim_excerpt: "C cites B".to_owned(),
                decision: DecisionState::Confirmed {
                    actor: analyst.clone(),
                    rationale: "r".to_owned(),
                },
            },
        ],
        developments: Vec::new(),
    };
    let report = evaluate_research_links(&two_cycle_req, || None)?;
    assert_eq!(report.citation_cycles.len(), 1);
    assert_eq!(
        report.publications_with_circular_dependency,
        vec![pub_a, pub_b, pub_c]
    );
    assert!(report.limitations.iter().any(|l| matches!(
        l,
        ResearchLinkLimitation::CitationCycleSearchTruncated {
            reported_cycles: 1,
            ..
        }
    )));

    // 6. Reverse-reachability pruning on a 256-node directed cycle (`MAX_ENTITIES = 256`)
    //    enumerates the single 256-node cycle without exhausting `MAX_CYCLE_DFS_STEPS`.
    let mut ring_entities = Vec::with_capacity(256);
    let mut ring_links = Vec::with_capacity(256);
    for idx in 0..256_usize {
        let pub_id = OpaqueId::new(format!("ent:pub-ring-{idx:03}"))?;
        ring_entities.push(ResearchEntity {
            entity_id: pub_id,
            kind: PublicEntityKind::Publication,
            primary_name: format!("Ring Paper {idx:03}"),
            alternate_names: Vec::new(),
            defining_source_version: v1.clone(),
        });
    }
    for idx in 0..256_usize {
        let next_idx = (idx + 1) % 256;
        ring_links.push(PublicLinkRecord {
            link_id: OpaqueId::new(format!("link:ring-{idx:03}"))?,
            from_endpoint: LinkEndpoint::Entity(ring_entities[idx].entity_id.clone()),
            to_endpoint: LinkEndpoint::Entity(ring_entities[next_idx].entity_id.clone()),
            relation: PublicLinkRelation::CitesPublication,
            source_version: v1.clone(),
            verbatim_excerpt: "Ring citation".to_owned(),
            decision: DecisionState::Confirmed {
                actor: analyst.clone(),
                rationale: "r".to_owned(),
            },
        });
    }
    let ring_req = ResearchLinkRequest {
        limits: ResearchLinkLimits::default(),
        sources: base_sources,
        entities: ring_entities,
        mentions: Vec::new(),
        links: ring_links,
        developments: Vec::new(),
    };
    let ring_report = evaluate_research_links(&ring_req, || None)?;
    assert_eq!(ring_report.citation_cycles.len(), 1);
    assert_eq!(ring_report.citation_cycles[0].publication_chain.len(), 257);
    assert_eq!(ring_report.publications_with_circular_dependency.len(), 256);
    assert!(ring_report.limitations.is_empty());

    // Zero limit is rejected.
    assert_eq!(
        ResearchLinkLimits::new(0, 16, 8, 16, 8, 16, 16, 16, 8, 8, 256, 4096, 65536, 1, 1024),
        Err(ResearchLinkError::InvalidLimit {
            limit: ResearchLinkLimitKind::Sources,
            supplied: 0,
            max_allowed: 128,
        })
    );

    Ok(())
}

#[test]
#[allow(clippy::too_many_lines)]
fn excludes_non_observational_origins_and_retracted_sources_and_propagates_upstream_reassessment()
-> Result<(), TestError> {
    let analyst = ActorId::new("actor:reviewer-a08")?;
    let v_upstream_corrected = VersionId::new("ver:upstream-v1")?;
    let v_upstream_v2 = VersionId::new("ver:upstream-v2")?;
    let v_derived = VersionId::new("ver:derived-v1")?;
    let v_retracted = VersionId::new("ver:retracted-lab-v1")?;
    let v_retracted_mid = VersionId::new("ver:retracted-mid-v1")?;
    let v_derived_from_retracted = VersionId::new("ver:derived-from-retracted-v1")?;
    let v_forecast = VersionId::new("ver:forecast-v1")?;
    let v_extracted_via_forecast = VersionId::new("ver:extracted-via-forecast-v1")?;
    let v_inference = VersionId::new("ver:inference-v1")?;
    let v_recreation = VersionId::new("ver:recreation-v1")?;
    let v_predictive = VersionId::new("ver:predictive-v1")?;

    let org_id = OpaqueId::new("ent:org-1")?;
    let grant_id = OpaqueId::new("ent:grant-1")?;

    let req = ResearchLinkRequest {
        limits: ResearchLinkLimits::default(),
        sources: vec![
            ResearchSourceRecord {
                source_id: OpaqueId::new("src:upstream")?,
                version_id: v_upstream_corrected.clone(),
                origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
                lifecycle: SourceLifecycleState::CorrectedBy {
                    replacement_version: v_upstream_v2.clone(),
                    summary: "Updated calibration table".to_owned(),
                },
                derived_from_versions: Vec::new(),
                citation_label: "Upstream Source v1".to_owned(),
            },
            ResearchSourceRecord {
                source_id: OpaqueId::new("src:derived")?,
                version_id: v_derived.clone(),
                origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
                lifecycle: SourceLifecycleState::Current,
                derived_from_versions: vec![v_upstream_corrected.clone()],
                citation_label: "Derived Source v1".to_owned(),
            },
            ResearchSourceRecord {
                source_id: OpaqueId::new("src:retracted")?,
                version_id: v_retracted.clone(),
                origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
                lifecycle: SourceLifecycleState::Retracted {
                    reason: "Sensor bridge fault invalidated test run".to_owned(),
                },
                derived_from_versions: Vec::new(),
                citation_label: "Retracted Lab Report".to_owned(),
            },
            // Intermediate retracted source: `v_derived_from_retracted (Current)` -> `v_retracted_mid (Retracted)` -> `v_upstream_corrected (CorrectedBy)`
            ResearchSourceRecord {
                source_id: OpaqueId::new("src:retracted-mid")?,
                version_id: v_retracted_mid.clone(),
                origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
                lifecycle: SourceLifecycleState::Retracted {
                    reason: "Intermediate processing pipeline retracted".to_owned(),
                },
                derived_from_versions: vec![v_upstream_corrected.clone()],
                citation_label: "Retracted Intermediate Dataset".to_owned(),
            },
            ResearchSourceRecord {
                source_id: OpaqueId::new("src:derived-from-retracted")?,
                version_id: v_derived_from_retracted.clone(),
                origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
                lifecycle: SourceLifecycleState::Current,
                derived_from_versions: vec![v_retracted_mid.clone()],
                citation_label: "Current Summary Derived From Retracted Dataset".to_owned(),
            },
            // Intermediate Forecast source: `v_extracted_via_forecast (ExtractedClaim)` -> `v_forecast (Forecast)` -> `v_upstream_corrected (AcquiredSourceMaterial)`
            ResearchSourceRecord {
                source_id: OpaqueId::new("src:forecast")?,
                version_id: v_forecast.clone(),
                origin_class: EvidenceOriginClass::Forecast,
                lifecycle: SourceLifecycleState::Current,
                derived_from_versions: vec![v_upstream_corrected.clone()],
                citation_label: "Forecast Scenario".to_owned(),
            },
            ResearchSourceRecord {
                source_id: OpaqueId::new("src:extracted-via-forecast")?,
                version_id: v_extracted_via_forecast.clone(),
                origin_class: EvidenceOriginClass::ExtractedClaim,
                lifecycle: SourceLifecycleState::Current,
                derived_from_versions: vec![v_forecast.clone()],
                citation_label: "Claim Extracted From Forecast Scenario".to_owned(),
            },
            ResearchSourceRecord {
                source_id: OpaqueId::new("src:inference")?,
                version_id: v_inference.clone(),
                origin_class: EvidenceOriginClass::AnalyticalInference,
                lifecycle: SourceLifecycleState::Current,
                derived_from_versions: Vec::new(),
                citation_label: "Analytical Inference Output".to_owned(),
            },
            ResearchSourceRecord {
                source_id: OpaqueId::new("src:recreation")?,
                version_id: v_recreation.clone(),
                origin_class: EvidenceOriginClass::IllustrativeRecreation,
                lifecycle: SourceLifecycleState::Current,
                derived_from_versions: Vec::new(),
                citation_label: "Illustrative Recreation".to_owned(),
            },
            ResearchSourceRecord {
                source_id: OpaqueId::new("src:predictive")?,
                version_id: v_predictive.clone(),
                origin_class: EvidenceOriginClass::PredictiveDerivative,
                lifecycle: SourceLifecycleState::Current,
                derived_from_versions: Vec::new(),
                citation_label: "Super-resolution Derivative".to_owned(),
            },
        ],
        entities: vec![
            ResearchEntity {
                entity_id: org_id.clone(),
                kind: PublicEntityKind::Organization,
                primary_name: "Test Org".to_owned(),
                alternate_names: Vec::new(),
                defining_source_version: v_derived.clone(),
            },
            ResearchEntity {
                entity_id: grant_id.clone(),
                kind: PublicEntityKind::Grant,
                primary_name: "Grant 1".to_owned(),
                alternate_names: Vec::new(),
                defining_source_version: v_derived.clone(),
            },
        ],
        mentions: Vec::new(),
        links: vec![
            PublicLinkRecord {
                link_id: OpaqueId::new("link:retracted-funding")?,
                from_endpoint: LinkEndpoint::Entity(grant_id),
                to_endpoint: LinkEndpoint::Entity(org_id.clone()),
                relation: PublicLinkRelation::GrantsFundingTo,
                source_version: v_retracted.clone(),
                verbatim_excerpt: "Retracted grant link".to_owned(),
                decision: DecisionState::Confirmed {
                    actor: analyst.clone(),
                    rationale: "Initial entry".to_owned(),
                },
            },
            PublicLinkRecord {
                link_id: OpaqueId::new("link:derived-affiliation")?,
                from_endpoint: LinkEndpoint::Entity(org_id.clone()),
                to_endpoint: LinkEndpoint::Entity(org_id.clone()),
                relation: PublicLinkRelation::AffiliatedWithOrganization,
                source_version: v_derived,
                verbatim_excerpt: "Derived affiliation".to_owned(),
                decision: DecisionState::Confirmed {
                    actor: analyst.clone(),
                    rationale: "Derived entry".to_owned(),
                },
            },
        ],
        developments: vec![TechnicalDevelopmentInput {
            development_id: OpaqueId::new("dev:non-observational-and-retracted")?,
            topic: "Non-observational and retracted items".to_owned(),
            subject_endpoint: LinkEndpoint::Entity(org_id),
            items: vec![
                TechnicalEvidenceItem {
                    item_id: OpaqueId::new("item:retracted")?,
                    artifact_kind: TechnicalArtifactKind::LaboratoryTestReport,
                    claimed_stage: ClaimedTechnicalStage::LaboratoryDemonstration,
                    publication_entity_id: None,
                    funding_or_contract_ids: Vec::new(),
                    source_version: v_retracted.clone(),
                    verbatim_claim: "Retracted lab claim".to_owned(),
                    stated_conditions_and_limitations: "Retracted".to_owned(),
                    decision: DecisionState::Confirmed {
                        actor: analyst.clone(),
                        rationale: "Confirmed before retraction".to_owned(),
                    },
                },
                TechnicalEvidenceItem {
                    item_id: OpaqueId::new("item:derived-from-retracted")?,
                    artifact_kind: TechnicalArtifactKind::LaboratoryTestReport,
                    claimed_stage: ClaimedTechnicalStage::LaboratoryDemonstration,
                    publication_entity_id: None,
                    funding_or_contract_ids: Vec::new(),
                    source_version: v_derived_from_retracted.clone(),
                    verbatim_claim: "Claim derived through retracted intermediate".to_owned(),
                    stated_conditions_and_limitations: "Upstream retracted".to_owned(),
                    decision: DecisionState::Confirmed {
                        actor: analyst.clone(),
                        rationale: "Confirmed before intermediate retraction".to_owned(),
                    },
                },
                TechnicalEvidenceItem {
                    item_id: OpaqueId::new("item:forecast")?,
                    artifact_kind: TechnicalArtifactKind::FieldOrOperationalTestReport,
                    claimed_stage: ClaimedTechnicalStage::FieldOrOperationalDemonstration,
                    publication_entity_id: None,
                    funding_or_contract_ids: Vec::new(),
                    source_version: v_forecast.clone(),
                    verbatim_claim: "Forecasted field capability".to_owned(),
                    stated_conditions_and_limitations: "Forecast".to_owned(),
                    decision: DecisionState::Confirmed {
                        actor: analyst.clone(),
                        rationale: "Confirmed".to_owned(),
                    },
                },
                TechnicalEvidenceItem {
                    item_id: OpaqueId::new("item:extracted-via-forecast")?,
                    artifact_kind: TechnicalArtifactKind::FieldOrOperationalTestReport,
                    claimed_stage: ClaimedTechnicalStage::FieldOrOperationalDemonstration,
                    publication_entity_id: None,
                    funding_or_contract_ids: Vec::new(),
                    source_version: v_extracted_via_forecast.clone(),
                    verbatim_claim: "Extracted claim from forecast".to_owned(),
                    stated_conditions_and_limitations: "Intermediate forecast".to_owned(),
                    decision: DecisionState::Confirmed {
                        actor: analyst.clone(),
                        rationale: "Confirmed".to_owned(),
                    },
                },
                TechnicalEvidenceItem {
                    item_id: OpaqueId::new("item:inference")?,
                    artifact_kind: TechnicalArtifactKind::FieldOrOperationalTestReport,
                    claimed_stage: ClaimedTechnicalStage::FieldOrOperationalDemonstration,
                    publication_entity_id: None,
                    funding_or_contract_ids: Vec::new(),
                    source_version: v_inference.clone(),
                    verbatim_claim: "Inferred field capability".to_owned(),
                    stated_conditions_and_limitations: "Inference".to_owned(),
                    decision: DecisionState::Confirmed {
                        actor: analyst.clone(),
                        rationale: "Confirmed".to_owned(),
                    },
                },
                TechnicalEvidenceItem {
                    item_id: OpaqueId::new("item:recreation")?,
                    artifact_kind: TechnicalArtifactKind::FieldOrOperationalTestReport,
                    claimed_stage: ClaimedTechnicalStage::FieldOrOperationalDemonstration,
                    publication_entity_id: None,
                    funding_or_contract_ids: Vec::new(),
                    source_version: v_recreation.clone(),
                    verbatim_claim: "Recreated visual".to_owned(),
                    stated_conditions_and_limitations: "Recreation".to_owned(),
                    decision: DecisionState::Confirmed {
                        actor: analyst.clone(),
                        rationale: "Confirmed".to_owned(),
                    },
                },
                TechnicalEvidenceItem {
                    item_id: OpaqueId::new("item:predictive")?,
                    artifact_kind: TechnicalArtifactKind::FieldOrOperationalTestReport,
                    claimed_stage: ClaimedTechnicalStage::FieldOrOperationalDemonstration,
                    publication_entity_id: None,
                    funding_or_contract_ids: Vec::new(),
                    source_version: v_predictive.clone(),
                    verbatim_claim: "Predictive derivative".to_owned(),
                    stated_conditions_and_limitations: "Super-resolution".to_owned(),
                    decision: DecisionState::Confirmed {
                        actor: analyst,
                        rationale: "Confirmed".to_owned(),
                    },
                },
            ],
        }],
    };

    let report = evaluate_research_links(&req, || None)?;

    // Verify `SourceSummary` explanations for intermediate Forecast and intermediate Retracted sources:
    let extracted_forecast_summary = report
        .sources
        .iter()
        .find(|s| s.version_id == v_extracted_via_forecast)
        .ok_or(ResearchLinkError::EmptySources)?;
    assert!(!extracted_forecast_summary.counts_as_reported_public_source);
    assert_eq!(
        extracted_forecast_summary.non_observational_ancestor_versions,
        vec![v_forecast.clone()]
    );

    let derived_retracted_summary = report
        .sources
        .iter()
        .find(|s| s.version_id == v_derived_from_retracted)
        .ok_or(ResearchLinkError::EmptySources)?;
    assert!(!derived_retracted_summary.counts_as_reported_public_source);
    assert_eq!(
        derived_retracted_summary.retracted_ancestor_versions,
        vec![v_retracted_mid.clone()]
    );

    // Retracted link is InactiveRetractedSource and does not count as confirmed funding.
    assert_eq!(
        report.links[0].effective_status,
        LinkEffectiveStatus::InactiveRetractedSource
    );
    assert!(report.links[0].needs_reassessment);
    assert!(
        report.links[0]
            .reassessment_reasons
            .contains(&ReassessmentReason::SourceRetracted {
                source_version: v_retracted.clone(),
            })
    );

    // Transitive upstream `CorrectedBy` on `v_upstream_corrected` propagates through `v_derived`
    // to `link:derived-affiliation` and `dev:non-observational-and-retracted`.
    assert!(report.links[1].needs_reassessment);
    assert!(
        report.links[1]
            .reassessment_reasons
            .contains(&ReassessmentReason::SourceCorrected {
                source_version: v_upstream_corrected.clone(),
                replacement_version: v_upstream_v2.clone(),
            })
    );

    let dev = &report.developments[0];
    // Neither direct/intermediate retracted lab reports nor direct/intermediate
    // Forecast/AnalyticalInference/IllustrativeRecreation/PredictiveDerivative items
    // can produce `ReportedDemonstratedCapability`.
    assert_eq!(
        dev.overall_maturity,
        OverallTechnicalMaturity::SimulatedOnly
    );
    assert_eq!(dev.retracted_item_count, 2);
    assert_eq!(dev.simulated_item_count, 5);
    assert_eq!(dev.confirmed_demonstration_item_count, 0);
    assert!(dev.confirmed_funding_or_contract_ids.is_empty());
    assert_eq!(
        dev.items[0].assessed_stage,
        AssessedTechnicalStage::RetractedSourceClaim
    );
    assert_eq!(
        dev.items[0].caveats,
        vec![
            DemonstrationCaveat::RetractedSourceCannotEstablishMaturity {
                source_version: v_retracted,
            }
        ]
    );
    // Intermediate retracted source names `v_retracted_mid` on the caveat even though
    // `item:derived-from-retracted` cites `v_derived_from_retracted` (`Current`).
    assert_eq!(
        dev.items[1].assessed_stage,
        AssessedTechnicalStage::RetractedSourceClaim
    );
    assert_eq!(
        dev.items[1].caveats,
        vec![
            DemonstrationCaveat::RetractedSourceCannotEstablishMaturity {
                source_version: v_retracted_mid,
            }
        ]
    );
    let expected_non_obs = [
        vec![(v_forecast.clone(), EvidenceOriginClass::Forecast)],
        vec![(v_forecast, EvidenceOriginClass::Forecast)],
        vec![(v_inference, EvidenceOriginClass::AnalyticalInference)],
        vec![(v_recreation, EvidenceOriginClass::IllustrativeRecreation)],
        vec![(v_predictive, EvidenceOriginClass::PredictiveDerivative)],
    ];
    for (item, expected_ancestors) in dev.items[2..].iter().zip(expected_non_obs) {
        assert_eq!(
            item.assessed_stage,
            AssessedTechnicalStage::SimulatedEvidenceOnly
        );
        assert_eq!(
            item.caveats,
            vec![
                DemonstrationCaveat::SimulatedSourceCannotEstablishDemonstration {
                    qualifying_ancestors: expected_ancestors,
                }
            ]
        );
    }
    assert!(dev.needs_reassessment);
    assert!(
        dev.reassessment_reasons
            .contains(&ReassessmentReason::SourceCorrected {
                source_version: v_upstream_corrected,
                replacement_version: v_upstream_v2,
            })
    );

    Ok(())
}
