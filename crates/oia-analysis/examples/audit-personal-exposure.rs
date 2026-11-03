//! Runnable synthetic demonstration of the user-scoped personal public exposure,
//! ambiguous account-connection matching, and owned-infrastructure audit kernel (`#47`, `PRIVACY-02`, `A18`).
//!
//! Uses only synthetic personas, reserved domains (`example.org`, `example.net`), and
//! documentation IP ranges (`192.0.2.0/24`, `2001:db8::/32`).

use oia_analysis::exposure_audit::{
    AccountMatchInput, AccountMatchReviewInput, AccountMatchSignal, AuditTimePrecision,
    AuditTimestamp, DeclaredAuditScope, DnsRecordType, ExposedContactKind, ExposureAuditLimits,
    ExposureCitationAnchor, InfrastructureArtifactKind, InfrastructureEntryInput,
    MatchAssessmentState, MatchSignalKind, ObservationTimeEnvelope, OutboundQueryTarget,
    OutboundSourceQueryCandidate, OwnershipBasis, PersonalExposureAudit, PublicProfileInput,
    SourceCitation, SourceCoverageRecord, SourceQueryCoverage,
};
use oia_domain::collaboration::{AccessSurface, CaseRole, ClearanceGrant};
use oia_domain::witness::AccountOrigin;
use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};

fn exact_utc(unix_ms: i64) -> AuditTimestamp {
    AuditTimestamp {
        unix_ms,
        precision: AuditTimePrecision::ExactInstant {
            original_tz_label: "UTC".to_owned(),
        },
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let owner = ActorId::new("actor:synthetic-owner")?;
    let limits = ExposureAuditLimits::default();

    let scope = DeclaredAuditScope::new(
        OpaqueId::new("scope:synthetic-personal-2026")?,
        VersionId::new("ver:scope-v1")?,
        1_730_500_000_000,
        OwnershipBasis::SelfOwnedPersonalScope {
            owner_actor: owner.clone(),
            verification_note: "Verified via local DNS TXT challenge on synthetic example.org."
                .to_owned(),
        },
        vec!["example.org".to_owned()],
        vec!["fictional_observer_01".to_owned()],
        limits,
    )?;

    let mut audit = PersonalExposureAudit::new(
        OpaqueId::new("audit:synthetic-exposure-01")?,
        SchemaVersion::V1,
        scope,
        limits,
    )?;

    let access = audit.evaluate_surface_access(
        CaseRole::Owner,
        ClearanceGrant::full(),
        AccessSurface::PrivateExport,
    )?;
    println!("proposed access decision for Owner PrivateExport: {access:?}");

    let permitted_query = audit.prepare_read_only_source_query(OutboundSourceQueryCandidate {
        source_id: OpaqueId::new("source:ct-log-fixture")?,
        target: OutboundQueryTarget::DeclaredDomain("obs.example.org".to_owned()),
        requested_at_unix_ms: 1_730_500_005_000,
        attaches_private_notes_or_hypotheses: false,
        exports_full_scope_manifest: false,
        mutates_remote_account_or_dns: false,
    })?;
    println!(
        "permitted read-only query target={} notice={}",
        permitted_query.normalized_target(),
        permitted_query.privacy_notice()
    );

    audit.register_public_profile(PublicProfileInput {
        profile_id: OpaqueId::new("prof:synthetic-primary")?,
        version_id: VersionId::new("ver:prof-primary-v1")?,
        origin: AccountOrigin::Simulated,
        service_name: "SyntheticCodeHost".to_owned(),
        handle: "fictional_observer_01".to_owned(),
        display_name: Some("Fictional Observer".to_owned()),
        bio_excerpt: Some("Synthetic sky tracking fixture profile.".to_owned()),
        listed_website_domain: Some("example.org".to_owned()),
        exposed_contact_values: vec!["ops@example.org".to_owned()],
        claimed_in_declared_scope: true,
        citation: SourceCitation {
            source_id: OpaqueId::new("source:synthetic-code-host")?,
            source_version: VersionId::new("ver:src-code-20261102")?,
            anchor: ExposureCitationAnchor::Utf8ByteRange(0..128),
        },
        timestamps: ObservationTimeEnvelope {
            observed_or_valid_from: Some(exact_utc(1_720_000_000_000)),
            published_or_issued_at: None,
            retrieved_at: exact_utc(1_730_500_010_000),
            audited_at: exact_utc(1_730_500_020_000),
        },
    })?;

    audit.register_public_profile(PublicProfileInput {
        profile_id: OpaqueId::new("prof:synthetic-candidate-forum")?,
        version_id: VersionId::new("ver:prof-forum-v1")?,
        origin: AccountOrigin::Simulated,
        service_name: "SyntheticRadioForum".to_owned(),
        handle: "fictional_observer_01_west".to_owned(),
        display_name: Some("F. Observer".to_owned()),
        bio_excerpt: Some("Synthetic amateur radio fixture bio.".to_owned()),
        listed_website_domain: None,
        exposed_contact_values: Vec::new(),
        claimed_in_declared_scope: false,
        citation: SourceCitation {
            source_id: OpaqueId::new("source:synthetic-forum")?,
            source_version: VersionId::new("ver:src-forum-20261102")?,
            anchor: ExposureCitationAnchor::ExtractedPassage {
                extraction_version: VersionId::new("ver:ext-forum-v1")?,
                passage_line: 12,
                byte_range: 40..112,
            },
        },
        timestamps: ObservationTimeEnvelope {
            observed_or_valid_from: None,
            published_or_issued_at: Some(AuditTimestamp {
                unix_ms: 1_715_040_000_000,
                precision: AuditTimePrecision::DateOnly {
                    calendar_date: "2024-05-07".to_owned(),
                },
            }),
            retrieved_at: exact_utc(1_730_500_030_000),
            audited_at: exact_utc(1_730_500_040_000),
        },
    })?;

    let candidate = audit.propose_account_match(AccountMatchInput {
        match_id: OpaqueId::new("match:primary-to-forum")?,
        version_id: VersionId::new("ver:match-v1")?,
        primary_profile_id: OpaqueId::new("prof:synthetic-primary")?,
        candidate_profile_id: OpaqueId::new("prof:synthetic-candidate-forum")?,
        uncalibrated_heuristic_score: 0.84,
        matching_signals: vec![
            AccountMatchSignal {
                kind: MatchSignalKind::SimilarHandlePattern,
                description: "Handle prefix `fictional_observer_01` matches declared handle."
                    .to_owned(),
                citation: SourceCitation {
                    source_id: OpaqueId::new("source:synthetic-forum")?,
                    source_version: VersionId::new("ver:src-forum-20261102")?,
                    anchor: ExposureCitationAnchor::StructuredRecordField {
                        record_locator: "member/4481".to_owned(),
                        field_name: "handle".to_owned(),
                    },
                },
            },
            AccountMatchSignal {
                kind: MatchSignalKind::BiographyPhraseOverlap,
                description: "Both synthetic bios mention observation fixtures.".to_owned(),
                citation: SourceCitation {
                    source_id: OpaqueId::new("source:synthetic-forum")?,
                    source_version: VersionId::new("ver:src-forum-20261102")?,
                    anchor: ExposureCitationAnchor::ExtractedPassage {
                        extraction_version: VersionId::new("ver:ext-forum-v1")?,
                        passage_line: 12,
                        byte_range: 40..112,
                    },
                },
            },
        ],
        contradicting_signals: vec![
            "No link back to example.org and no shared PGP key.".to_owned(),
        ],
        ambiguity_explanation: "Synthetic handle prefix overlap; could be an unrelated account."
            .to_owned(),
        proposed_at: exact_utc(1_730_500_050_000),
    })?;
    println!(
        "initial candidate match state={:?} automated_identity={}",
        candidate.assessment_state(),
        candidate
            .assessment_state()
            .establishes_automated_identity()
    );

    let reviewed = audit.review_account_match(AccountMatchReviewInput {
        match_id: OpaqueId::new("match:primary-to-forum")?,
        expected_version: VersionId::new("ver:match-v1")?,
        new_version: VersionId::new("ver:match-v2")?,
        reviewer: owner,
        reviewed_at: exact_utc(1_730_500_060_000),
        new_state: MatchAssessmentState::AmbiguousRetained,
        private_rationale_note: "Retaining as ambiguous in synthetic audit demonstration."
            .to_owned(),
    })?;
    println!(
        "after review state={:?} history_len={}",
        reviewed.assessment_state(),
        reviewed.review_history().len()
    );

    audit.record_infrastructure_entry(InfrastructureEntryInput {
        entry_id: OpaqueId::new("infra:dns-obs-a")?,
        version_id: VersionId::new("ver:infra-dns-v1")?,
        origin: AccountOrigin::Simulated,
        domain: "obs.example.org".to_owned(),
        artifact: InfrastructureArtifactKind::DnsRecord {
            record_type: DnsRecordType::A,
            record_value: "192.0.2.42".to_owned(),
            ttl_seconds: Some(3600),
        },
        citation: SourceCitation {
            source_id: OpaqueId::new("source:dns-snapshot-fixture")?,
            source_version: VersionId::new("ver:dns-20261102")?,
            anchor: ExposureCitationAnchor::StructuredRecordField {
                record_locator: "zone/example.org#rr-12".to_owned(),
                field_name: "rdata".to_owned(),
            },
        },
        timestamps: ObservationTimeEnvelope {
            observed_or_valid_from: Some(exact_utc(1_730_490_000_000)),
            published_or_issued_at: Some(exact_utc(1_730_490_000_000)),
            retrieved_at: exact_utc(1_730_500_070_000),
            audited_at: exact_utc(1_730_500_080_000),
        },
    })?;

    audit.record_infrastructure_entry(InfrastructureEntryInput {
        entry_id: OpaqueId::new("infra:ct-cert-01")?,
        version_id: VersionId::new("ver:infra-ct-v1")?,
        origin: AccountOrigin::Simulated,
        domain: "example.org".to_owned(),
        artifact: InfrastructureArtifactKind::CertificateHistory {
            certificate_sha256_fingerprint:
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_owned(),
            serial_number: "04:a1:9c:22:ff".to_owned(),
            issuer_common_name: "Synthetic Example CA R3".to_owned(),
            subject_common_name: "example.org".to_owned(),
            subject_alt_names: vec!["example.org".to_owned(), "obs.example.org".to_owned()],
            not_before_unix_ms: 1_725_000_000_000,
            not_after_unix_ms: 1_732_776_000_000,
        },
        citation: SourceCitation {
            source_id: OpaqueId::new("source:ct-log-fixture")?,
            source_version: VersionId::new("ver:ct-entry-99102")?,
            anchor: ExposureCitationAnchor::StructuredRecordField {
                record_locator: "ct-log#index-99102".to_owned(),
                field_name: "leaf_certificate".to_owned(),
            },
        },
        timestamps: ObservationTimeEnvelope {
            observed_or_valid_from: Some(exact_utc(1_725_000_000_000)),
            published_or_issued_at: Some(exact_utc(1_725_000_005_000)),
            retrieved_at: exact_utc(1_730_500_090_000),
            audited_at: exact_utc(1_730_500_100_000),
        },
    })?;

    audit.record_infrastructure_entry(InfrastructureEntryInput {
        entry_id: OpaqueId::new("infra:contact-soa")?,
        version_id: VersionId::new("ver:infra-contact-v1")?,
        origin: AccountOrigin::Simulated,
        domain: "example.org".to_owned(),
        artifact: InfrastructureArtifactKind::ExposedContact {
            contact_kind: ExposedContactKind::EmailAddress,
            exposed_value: "hostmaster@example.org".to_owned(),
            context_location: "Synthetic WHOIS / SOA RNAME record".to_owned(),
            remediation_note: "Replace personal mailbox in SOA RNAME with role alias.".to_owned(),
        },
        citation: SourceCitation {
            source_id: OpaqueId::new("source:dns-snapshot-fixture")?,
            source_version: VersionId::new("ver:dns-20261102")?,
            anchor: ExposureCitationAnchor::StructuredRecordField {
                record_locator: "zone/example.org#soa".to_owned(),
                field_name: "rname".to_owned(),
            },
        },
        timestamps: ObservationTimeEnvelope {
            observed_or_valid_from: Some(exact_utc(1_700_000_000_000)),
            published_or_issued_at: None,
            retrieved_at: exact_utc(1_730_500_070_000),
            audited_at: exact_utc(1_730_500_085_000),
        },
    })?;

    audit.record_source_coverage(SourceCoverageRecord {
        coverage_id: OpaqueId::new("cov:ct-query-01")?,
        source_id: OpaqueId::new("source:ct-log-fixture")?,
        source_version: VersionId::new("ver:ct-entry-99102")?,
        queried_target: "example.org".to_owned(),
        queried_at: exact_utc(1_730_500_090_000),
        coverage: SourceQueryCoverage::PartialBudgetReached {
            returned_items: 1,
            omission_note: "Historical CT page limit set to 1 entry for synthetic check."
                .to_owned(),
        },
    })?;

    let summary = audit.summary();
    println!("exposure audit summary: {summary:?}");
    Ok(())
}
