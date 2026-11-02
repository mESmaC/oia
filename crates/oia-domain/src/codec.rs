//! Canonical JSON serialization and deserialization for `DomainObject`, `ObjectEnvelope`,
//! and `RelationEdge` (`A1`, `D02`, `docs/architecture.md` Section 2.1).
//!
//! Uses `oia-domain::json::JsonValue` (resource-bounded parser and deterministic `BTreeMap`
//! serializer) with zero external crate dependencies.

use std::collections::BTreeMap;

use crate::graph::{RelationEdge, RelationKind};
use crate::ids::{ActorId, OpaqueId, SchemaVersion, VersionId};
use crate::json::{JsonError, JsonValue};
use crate::records::{
    Annotation, Artifact, AvailabilityState, CitationAnchor, Claim, ClaimAssessment, ClaimNature,
    Document, DocumentReleaseKind, DomainError, DomainObject, DomainObjectType, Event,
    ExtractionCoverage, Hypothesis, Investigation, ObjectEnvelope, Observation, OmissionReason,
    OriginClass, ProvenanceRecord, ReassessmentState, RecordedValue, Report, ReportBlock,
    ReportPublicationState, ResearchLead, RetainedContent, ReuseConditions,
    ReversibleClusterDecision, Review, ReviewStatus, SavedQuery, Source, SourceHealthState,
    SourceRecord, WorkspaceVisibility,
};
use crate::sha256::ContentHash;

fn str_arr(items: &[String]) -> JsonValue {
    JsonValue::Array(items.iter().cloned().map(JsonValue::Str).collect())
}

fn ver_arr(items: &[VersionId]) -> JsonValue {
    JsonValue::Array(
        items
            .iter()
            .map(|v| JsonValue::Str(v.as_str().to_owned()))
            .collect(),
    )
}

fn parse_str_vec(obj: &BTreeMap<String, JsonValue>, key: &str) -> Result<Vec<String>, DomainError> {
    let arr = JsonValue::req_array(obj, key)?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        match item {
            JsonValue::Str(s) => out.push(s.clone()),
            _ => {
                return Err(DomainError::Json(JsonError::Schema(format!(
                    "elements of '{key}' must be strings"
                ))));
            }
        }
    }
    Ok(out)
}

fn parse_ver_vec(
    obj: &BTreeMap<String, JsonValue>,
    key: &str,
) -> Result<Vec<VersionId>, DomainError> {
    let arr = JsonValue::req_array(obj, key)?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        match item {
            JsonValue::Str(s) => out.push(VersionId::new(s.clone())?),
            _ => {
                return Err(DomainError::Json(JsonError::Schema(format!(
                    "elements of '{key}' must be VersionId strings"
                ))));
            }
        }
    }
    Ok(out)
}

impl SourceHealthState {
    /// Machine-readable tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unconfigured => "unconfigured",
            Self::Connecting => "connecting",
            Self::Available => "available",
            Self::Degraded => "degraded",
            Self::Stale => "stale",
            Self::Unavailable => "unavailable",
            Self::Disabled => "disabled",
        }
    }

    /// Parses a machine-readable tag.
    pub fn from_tag(tag: &str) -> Result<Self, DomainError> {
        match tag {
            "unconfigured" => Ok(Self::Unconfigured),
            "connecting" => Ok(Self::Connecting),
            "available" => Ok(Self::Available),
            "degraded" => Ok(Self::Degraded),
            "stale" => Ok(Self::Stale),
            "unavailable" => Ok(Self::Unavailable),
            "disabled" => Ok(Self::Disabled),
            other => Err(DomainError::InvalidEnumTag {
                field: "source_health",
                value: other.to_owned(),
            }),
        }
    }
}

impl OmissionReason {
    /// Machine-readable tag.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::RetentionProhibitedByPolicy => "retention_prohibited_by_policy",
            Self::RedistributionProhibitedByPolicy => "redistribution_prohibited_by_policy",
            Self::RedactedSensitiveContent => "redacted_sensitive_content",
            Self::UpstreamReferenceOnly => "upstream_reference_only",
        }
    }

    /// Parses a machine-readable tag.
    pub fn from_tag(tag: &str) -> Result<Self, DomainError> {
        match tag {
            "retention_prohibited_by_policy" => Ok(Self::RetentionProhibitedByPolicy),
            "redistribution_prohibited_by_policy" => Ok(Self::RedistributionProhibitedByPolicy),
            "redacted_sensitive_content" => Ok(Self::RedactedSensitiveContent),
            "upstream_reference_only" => Ok(Self::UpstreamReferenceOnly),
            other => Err(DomainError::InvalidEnumTag {
                field: "omission_reason",
                value: other.to_owned(),
            }),
        }
    }
}

impl ReportPublicationState {
    /// Machine-readable tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PrivateDraft => "private_draft",
            Self::InReview => "in_review",
            Self::PrivateExported => "private_exported",
            Self::Published => "published",
        }
    }

    /// Parses a machine-readable tag.
    pub fn from_tag(tag: &str) -> Result<Self, DomainError> {
        match tag {
            "private_draft" => Ok(Self::PrivateDraft),
            "in_review" => Ok(Self::InReview),
            "private_exported" => Ok(Self::PrivateExported),
            "published" => Ok(Self::Published),
            other => Err(DomainError::InvalidEnumTag {
                field: "publication_state",
                value: other.to_owned(),
            }),
        }
    }
}

impl RelationKind {
    /// Machine-readable tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DerivedFrom => "derived_from",
            Self::Quotes => "quotes",
            Self::Republishes => "republishes",
            Self::Supports => "supports",
            Self::Contradicts => "contradicts",
            Self::PossibleSameEvent => "possible_same_event",
            Self::Supersedes => "supersedes",
            Self::CalibratedBy => "calibrated_by",
            Self::CoOccursWith => "co_occurs_with",
        }
    }

    /// Parses a machine-readable tag.
    pub fn from_tag(tag: &str) -> Result<Self, DomainError> {
        match tag {
            "derived_from" => Ok(Self::DerivedFrom),
            "quotes" => Ok(Self::Quotes),
            "republishes" => Ok(Self::Republishes),
            "supports" => Ok(Self::Supports),
            "contradicts" => Ok(Self::Contradicts),
            "possible_same_event" => Ok(Self::PossibleSameEvent),
            "supersedes" => Ok(Self::Supersedes),
            "calibrated_by" => Ok(Self::CalibratedBy),
            "co_occurs_with" => Ok(Self::CoOccursWith),
            other => Err(DomainError::InvalidEnumTag {
                field: "relation_kind",
                value: other.to_owned(),
            }),
        }
    }
}

impl RelationEdge {
    /// Serializes this edge to a canonical [`JsonValue`].
    #[must_use]
    pub fn to_json_value(&self) -> JsonValue {
        let mut m = BTreeMap::new();
        m.insert(
            "edge_id".to_owned(),
            JsonValue::Str(self.edge_id.as_str().to_owned()),
        );
        m.insert(
            "from_version".to_owned(),
            JsonValue::Str(self.from_version.as_str().to_owned()),
        );
        m.insert(
            "to_version".to_owned(),
            JsonValue::Str(self.to_version.as_str().to_owned()),
        );
        m.insert(
            "kind".to_owned(),
            JsonValue::Str(self.kind.as_str().to_owned()),
        );
        m.insert(
            "rationale".to_owned(),
            JsonValue::Str(self.rationale.clone()),
        );
        m.insert(
            "review_status".to_owned(),
            JsonValue::Str(self.review_status.as_str().to_owned()),
        );
        JsonValue::Object(m)
    }

    /// Deserializes a [`RelationEdge`] from a [`JsonValue`].
    pub fn from_json_value(val: &JsonValue) -> Result<Self, DomainError> {
        let m = val.as_object()?;
        Ok(Self {
            edge_id: OpaqueId::new(JsonValue::req_str(m, "edge_id")?)?,
            from_version: VersionId::new(JsonValue::req_str(m, "from_version")?)?,
            to_version: VersionId::new(JsonValue::req_str(m, "to_version")?)?,
            kind: RelationKind::from_tag(JsonValue::req_str(m, "kind")?)?,
            rationale: JsonValue::req_str(m, "rationale")?.to_owned(),
            review_status: ReviewStatus::from_tag(JsonValue::req_str(m, "review_status")?)?,
        })
    }
}

impl ObjectEnvelope {
    /// Serializes this envelope to a canonical [`JsonValue`].
    #[must_use]
    pub fn to_json_value(&self) -> JsonValue {
        let mut m = BTreeMap::new();
        m.insert("id".to_owned(), JsonValue::Str(self.id.as_str().to_owned()));
        m.insert(
            "version_id".to_owned(),
            JsonValue::Str(self.version_id.as_str().to_owned()),
        );
        m.insert(
            "schema_version".to_owned(),
            JsonValue::Int(i64::from(self.schema_version.as_u32())),
        );
        m.insert(
            "object_type".to_owned(),
            JsonValue::Str(self.object_type.as_str().to_owned()),
        );
        m.insert(
            "created_at_ms".to_owned(),
            JsonValue::Int(self.created_at_ms),
        );
        m.insert(
            "created_by".to_owned(),
            JsonValue::Str(self.created_by.as_str().to_owned()),
        );
        m.insert("visibility".to_owned(), encode_visibility(&self.visibility));
        m.insert(
            "supersedes".to_owned(),
            match &self.supersedes {
                Some(v) => JsonValue::Str(v.as_str().to_owned()),
                None => JsonValue::Null,
            },
        );
        let mut prov = BTreeMap::new();
        prov.insert(
            "source_version_ids".to_owned(),
            ver_arr(&self.provenance.source_version_ids),
        );
        prov.insert(
            "declared_origin_note".to_owned(),
            match &self.provenance.declared_origin_note {
                Some(s) => JsonValue::Str(s.clone()),
                None => JsonValue::Null,
            },
        );
        prov.insert(
            "processing_history".to_owned(),
            str_arr(&self.provenance.processing_history),
        );
        m.insert("provenance".to_owned(), JsonValue::Object(prov));
        m.insert(
            "origin_class".to_owned(),
            JsonValue::Str(self.origin_class.as_str().to_owned()),
        );
        m.insert(
            "review_status".to_owned(),
            JsonValue::Str(self.review_status.as_str().to_owned()),
        );
        m.insert(
            "availability".to_owned(),
            JsonValue::Str(self.availability.as_str().to_owned()),
        );
        m.insert(
            "reassessment".to_owned(),
            encode_reassessment(&self.reassessment),
        );
        JsonValue::Object(m)
    }

    /// Deserializes an [`ObjectEnvelope`] from a [`JsonValue`].
    pub fn from_json_value(val: &JsonValue) -> Result<Self, DomainError> {
        let m = val.as_object()?;
        let sv_i64 = JsonValue::req_i64(m, "schema_version")?;
        let sv_u32 = u32::try_from(sv_i64).map_err(|_| {
            DomainError::Json(JsonError::Schema("invalid schema_version".to_owned()))
        })?;
        let prov_m = JsonValue::req_obj(m, "provenance")?;
        let env = Self {
            id: OpaqueId::new(JsonValue::req_str(m, "id")?)?,
            version_id: VersionId::new(JsonValue::req_str(m, "version_id")?)?,
            schema_version: SchemaVersion::new(sv_u32)?,
            object_type: DomainObjectType::from_tag(JsonValue::req_str(m, "object_type")?)?,
            created_at_ms: JsonValue::req_i64(m, "created_at_ms")?,
            created_by: ActorId::new(JsonValue::req_str(m, "created_by")?)?,
            visibility: decode_visibility(JsonValue::req_obj(m, "visibility")?)?,
            supersedes: match JsonValue::opt_str(m, "supersedes")? {
                Some(s) => Some(VersionId::new(s)?),
                None => None,
            },
            provenance: ProvenanceRecord {
                source_version_ids: parse_ver_vec(prov_m, "source_version_ids")?,
                declared_origin_note: JsonValue::opt_str(prov_m, "declared_origin_note")?,
                processing_history: parse_str_vec(prov_m, "processing_history")?,
            },
            origin_class: OriginClass::from_tag(JsonValue::req_str(m, "origin_class")?)?,
            review_status: ReviewStatus::from_tag(JsonValue::req_str(m, "review_status")?)?,
            availability: AvailabilityState::from_tag(JsonValue::req_str(m, "availability")?)?,
            reassessment: decode_reassessment(JsonValue::req_obj(m, "reassessment")?)?,
        };
        env.validate()?;
        Ok(env)
    }
}

fn encode_visibility(vis: &WorkspaceVisibility) -> JsonValue {
    let mut m = BTreeMap::new();
    match vis {
        WorkspaceVisibility::PrivateCase { case_id, sensitive } => {
            m.insert("kind".to_owned(), JsonValue::Str("private_case".to_owned()));
            m.insert(
                "case_id".to_owned(),
                JsonValue::Str(case_id.as_str().to_owned()),
            );
            m.insert("sensitive".to_owned(), JsonValue::Bool(*sensitive));
        }
        WorkspaceVisibility::SharedCase {
            case_id,
            policy_id,
            sensitive,
        } => {
            m.insert("kind".to_owned(), JsonValue::Str("shared_case".to_owned()));
            m.insert(
                "case_id".to_owned(),
                JsonValue::Str(case_id.as_str().to_owned()),
            );
            m.insert(
                "policy_id".to_owned(),
                JsonValue::Str(policy_id.as_str().to_owned()),
            );
            m.insert("sensitive".to_owned(), JsonValue::Bool(*sensitive));
        }
        WorkspaceVisibility::LibraryPublic => {
            m.insert(
                "kind".to_owned(),
                JsonValue::Str("library_public".to_owned()),
            );
        }
    }
    JsonValue::Object(m)
}

fn decode_visibility(m: &BTreeMap<String, JsonValue>) -> Result<WorkspaceVisibility, DomainError> {
    match JsonValue::req_str(m, "kind")? {
        "private_case" => Ok(WorkspaceVisibility::PrivateCase {
            case_id: OpaqueId::new(JsonValue::req_str(m, "case_id")?)?,
            sensitive: JsonValue::req_bool(m, "sensitive")?,
        }),
        "shared_case" => Ok(WorkspaceVisibility::SharedCase {
            case_id: OpaqueId::new(JsonValue::req_str(m, "case_id")?)?,
            policy_id: OpaqueId::new(JsonValue::req_str(m, "policy_id")?)?,
            sensitive: JsonValue::req_bool(m, "sensitive")?,
        }),
        "library_public" => Ok(WorkspaceVisibility::LibraryPublic),
        other => Err(DomainError::InvalidEnumTag {
            field: "visibility.kind",
            value: other.to_owned(),
        }),
    }
}

fn encode_reassessment(r: &ReassessmentState) -> JsonValue {
    let mut m = BTreeMap::new();
    match r {
        ReassessmentState::Current => {
            m.insert("state".to_owned(), JsonValue::Str("current".to_owned()));
        }
        ReassessmentState::NeedsReassessment {
            upstream_versions,
            reasons,
            flagged_at_ms,
        } => {
            m.insert(
                "state".to_owned(),
                JsonValue::Str("needs_reassessment".to_owned()),
            );
            m.insert("upstream_versions".to_owned(), ver_arr(upstream_versions));
            m.insert("reasons".to_owned(), str_arr(reasons));
            m.insert("flagged_at_ms".to_owned(), JsonValue::Int(*flagged_at_ms));
        }
    }
    JsonValue::Object(m)
}

fn decode_reassessment(m: &BTreeMap<String, JsonValue>) -> Result<ReassessmentState, DomainError> {
    match JsonValue::req_str(m, "state")? {
        "current" => Ok(ReassessmentState::Current),
        "needs_reassessment" => Ok(ReassessmentState::NeedsReassessment {
            upstream_versions: parse_ver_vec(m, "upstream_versions")?,
            reasons: parse_str_vec(m, "reasons")?,
            flagged_at_ms: JsonValue::req_i64(m, "flagged_at_ms")?,
        }),
        other => Err(DomainError::InvalidEnumTag {
            field: "reassessment.state",
            value: other.to_owned(),
        }),
    }
}

fn encode_anchor(a: &CitationAnchor) -> JsonValue {
    let mut m = BTreeMap::new();
    m.insert(
        "anchor_id".to_owned(),
        JsonValue::Str(a.anchor_id.as_str().to_owned()),
    );
    m.insert(
        "document_version_id".to_owned(),
        JsonValue::Str(a.document_version_id.as_str().to_owned()),
    );
    m.insert(
        "page_number".to_owned(),
        match a.page_number {
            Some(p) => JsonValue::Int(i64::from(p)),
            None => JsonValue::Null,
        },
    );
    m.insert("byte_start".to_owned(), JsonValue::Int(a.byte_start as i64));
    m.insert("byte_end".to_owned(), JsonValue::Int(a.byte_end as i64));
    m.insert(
        "span_hash".to_owned(),
        match &a.span_hash {
            Some(h) => JsonValue::Str(h.to_hex()),
            None => JsonValue::Null,
        },
    );
    let mut cov = BTreeMap::new();
    match &a.coverage {
        ExtractionCoverage::Complete => {
            cov.insert("kind".to_owned(), JsonValue::Str("complete".to_owned()));
        }
        ExtractionCoverage::Partial { limitation } => {
            cov.insert("kind".to_owned(), JsonValue::Str("partial".to_owned()));
            cov.insert("limitation".to_owned(), JsonValue::Str(limitation.clone()));
        }
        ExtractionCoverage::Unreadable { reason } => {
            cov.insert("kind".to_owned(), JsonValue::Str("unreadable".to_owned()));
            cov.insert("reason".to_owned(), JsonValue::Str(reason.clone()));
        }
    }
    m.insert("coverage".to_owned(), JsonValue::Object(cov));
    JsonValue::Object(m)
}

fn decode_anchor(val: &JsonValue) -> Result<CitationAnchor, DomainError> {
    let m = val.as_object()?;
    let cov_m = JsonValue::req_obj(m, "coverage")?;
    let coverage = match JsonValue::req_str(cov_m, "kind")? {
        "complete" => ExtractionCoverage::Complete,
        "partial" => ExtractionCoverage::Partial {
            limitation: JsonValue::req_str(cov_m, "limitation")?.to_owned(),
        },
        "unreadable" => ExtractionCoverage::Unreadable {
            reason: JsonValue::req_str(cov_m, "reason")?.to_owned(),
        },
        other => {
            return Err(DomainError::InvalidEnumTag {
                field: "coverage.kind",
                value: other.to_owned(),
            });
        }
    };
    let bs = usize::try_from(JsonValue::req_i64(m, "byte_start")?)
        .map_err(|_| DomainError::Json(JsonError::Schema("negative byte_start".to_owned())))?;
    let be = usize::try_from(JsonValue::req_i64(m, "byte_end")?)
        .map_err(|_| DomainError::Json(JsonError::Schema("negative byte_end".to_owned())))?;
    let page_number =
        match JsonValue::opt_i64(m, "page_number")? {
            Some(n) => Some(u32::try_from(n).map_err(|_| {
                DomainError::Json(JsonError::Schema("invalid page_number".to_owned()))
            })?),
            None => None,
        };
    let span_hash = match JsonValue::opt_str(m, "span_hash")? {
        Some(h) => Some(ContentHash::from_hex(&h)?),
        None => None,
    };
    let anchor = CitationAnchor {
        anchor_id: OpaqueId::new(JsonValue::req_str(m, "anchor_id")?)?,
        document_version_id: VersionId::new(JsonValue::req_str(m, "document_version_id")?)?,
        page_number,
        byte_start: bs,
        byte_end: be,
        span_hash,
        coverage,
    };
    anchor.validate()?;
    Ok(anchor)
}

impl DomainObject {
    /// Serializes this [`DomainObject`] into a canonical [`JsonValue`] (`A1`).
    pub fn to_json_value(&self) -> Result<JsonValue, DomainError> {
        let mut root = BTreeMap::new();
        root.insert("envelope".to_owned(), self.envelope().to_json_value());
        let mut body = BTreeMap::new();
        match self {
            Self::Source(s) => {
                root.insert("kind".to_owned(), JsonValue::Str("source".to_owned()));
                body.insert(
                    "provider_name".to_owned(),
                    JsonValue::Str(s.provider_name.clone()),
                );
                body.insert("capabilities".to_owned(), str_arr(&s.capabilities));
                let mut rc = BTreeMap::new();
                rc.insert(
                    "ordinary_access_method".to_owned(),
                    JsonValue::Str(s.reuse_conditions.ordinary_access_method.clone()),
                );
                rc.insert(
                    "automated_processing_allowed".to_owned(),
                    JsonValue::Bool(s.reuse_conditions.automated_processing_allowed),
                );
                rc.insert(
                    "retention_allowed".to_owned(),
                    JsonValue::Bool(s.reuse_conditions.retention_allowed),
                );
                rc.insert(
                    "redistribution_allowed".to_owned(),
                    JsonValue::Bool(s.reuse_conditions.redistribution_allowed),
                );
                rc.insert(
                    "attribution".to_owned(),
                    JsonValue::Str(s.reuse_conditions.attribution.clone()),
                );
                body.insert("reuse_conditions".to_owned(), JsonValue::Object(rc));
                body.insert(
                    "credential_ref".to_owned(),
                    match &s.credential_ref {
                        Some(c) => JsonValue::Str(c.as_str().to_owned()),
                        None => JsonValue::Null,
                    },
                );
                body.insert(
                    "health".to_owned(),
                    JsonValue::Str(s.health.as_str().to_owned()),
                );
            }
            Self::SourceRecord(sr) => {
                root.insert(
                    "kind".to_owned(),
                    JsonValue::Str("source_record".to_owned()),
                );
                body.insert(
                    "source_id".to_owned(),
                    JsonValue::Str(sr.source_id.as_str().to_owned()),
                );
                body.insert(
                    "upstream_version".to_owned(),
                    JsonValue::Str(sr.upstream_version.clone()),
                );
                body.insert(
                    "acquisition_attempt_id".to_owned(),
                    JsonValue::Str(sr.acquisition_attempt_id.as_str().to_owned()),
                );
                body.insert(
                    "source_locator".to_owned(),
                    JsonValue::Str(sr.source_locator.clone()),
                );
                body.insert(
                    "source_event_time_ms".to_owned(),
                    sr.source_event_time_ms
                        .map_or(JsonValue::Null, JsonValue::Int),
                );
                body.insert(
                    "public_availability_time_ms".to_owned(),
                    sr.public_availability_time_ms
                        .map_or(JsonValue::Null, JsonValue::Int),
                );
                body.insert(
                    "retrieved_at_ms".to_owned(),
                    JsonValue::Int(sr.retrieved_at_ms),
                );
                let mut content_m = BTreeMap::new();
                match &sr.content {
                    RetainedContent::RetainedBlob {
                        content_hash,
                        byte_len,
                        media_type,
                    } => {
                        content_m.insert(
                            "kind".to_owned(),
                            JsonValue::Str("retained_blob".to_owned()),
                        );
                        content_m.insert(
                            "content_hash".to_owned(),
                            JsonValue::Str(content_hash.to_hex()),
                        );
                        content_m.insert("byte_len".to_owned(), JsonValue::Int(*byte_len as i64));
                        content_m
                            .insert("media_type".to_owned(), JsonValue::Str(media_type.clone()));
                    }
                    RetainedContent::Omitted {
                        reason,
                        known_upstream_hash,
                    } => {
                        content_m.insert("kind".to_owned(), JsonValue::Str("omitted".to_owned()));
                        content_m.insert(
                            "reason".to_owned(),
                            JsonValue::Str(reason.as_str().to_owned()),
                        );
                        content_m.insert(
                            "known_upstream_hash".to_owned(),
                            match known_upstream_hash {
                                Some(h) => JsonValue::Str(h.to_hex()),
                                None => JsonValue::Null,
                            },
                        );
                    }
                }
                body.insert("content".to_owned(), JsonValue::Object(content_m));
            }
            Self::Artifact(art) => {
                root.insert("kind".to_owned(), JsonValue::Str("artifact".to_owned()));
                body.insert(
                    "content_hash".to_owned(),
                    JsonValue::Str(art.content_hash.to_hex()),
                );
                body.insert(
                    "media_type".to_owned(),
                    JsonValue::Str(art.media_type.clone()),
                );
                body.insert("byte_len".to_owned(), JsonValue::Int(art.byte_len as i64));
                body.insert(
                    "derived_from_artifact".to_owned(),
                    match &art.derived_from_artifact {
                        Some(v) => JsonValue::Str(v.as_str().to_owned()),
                        None => JsonValue::Null,
                    },
                );
                body.insert(
                    "quality_effect_disclosure".to_owned(),
                    match &art.quality_effect_disclosure {
                        Some(s) => JsonValue::Str(s.clone()),
                        None => JsonValue::Null,
                    },
                );
            }
            Self::Document(doc) => {
                root.insert("kind".to_owned(), JsonValue::Str("document".to_owned()));
                body.insert(
                    "source_record_version".to_owned(),
                    JsonValue::Str(doc.source_record_version.as_str().to_owned()),
                );
                body.insert(
                    "original_artifact_version".to_owned(),
                    match &doc.original_artifact_version {
                        Some(v) => JsonValue::Str(v.as_str().to_owned()),
                        None => JsonValue::Null,
                    },
                );
                body.insert("title".to_owned(), JsonValue::Str(doc.title.clone()));
                body.insert("authors".to_owned(), str_arr(&doc.authors));
                body.insert(
                    "publisher".to_owned(),
                    JsonValue::Str(doc.publisher.clone()),
                );
                body.insert("language".to_owned(), JsonValue::Str(doc.language.clone()));
                let mut rk = BTreeMap::new();
                match &doc.release_kind {
                    DocumentReleaseKind::OriginalRelease => {
                        rk.insert(
                            "kind".to_owned(),
                            JsonValue::Str("original_release".to_owned()),
                        );
                    }
                    DocumentReleaseKind::Revision {
                        previous_document_version,
                    } => {
                        rk.insert("kind".to_owned(), JsonValue::Str("revision".to_owned()));
                        rk.insert(
                            "previous_document_version".to_owned(),
                            JsonValue::Str(previous_document_version.as_str().to_owned()),
                        );
                    }
                    DocumentReleaseKind::DifferentlyRedactedRelease {
                        related_document_version,
                        redaction_note,
                    } => {
                        rk.insert(
                            "kind".to_owned(),
                            JsonValue::Str("differently_redacted_release".to_owned()),
                        );
                        rk.insert(
                            "related_document_version".to_owned(),
                            JsonValue::Str(related_document_version.as_str().to_owned()),
                        );
                        rk.insert(
                            "redaction_note".to_owned(),
                            JsonValue::Str(redaction_note.clone()),
                        );
                    }
                    DocumentReleaseKind::SyndicatedOrRepublished {
                        upstream_source_version,
                    } => {
                        rk.insert(
                            "kind".to_owned(),
                            JsonValue::Str("syndicated_or_republished".to_owned()),
                        );
                        rk.insert(
                            "upstream_source_version".to_owned(),
                            match upstream_source_version {
                                Some(v) => JsonValue::Str(v.as_str().to_owned()),
                                None => JsonValue::Null,
                            },
                        );
                    }
                }
                body.insert("release_kind".to_owned(), JsonValue::Object(rk));
                let mut passages_m = BTreeMap::new();
                for (k, v) in &doc.passages {
                    passages_m.insert(k.as_str().to_owned(), JsonValue::Str(v.clone()));
                }
                body.insert("passages".to_owned(), JsonValue::Object(passages_m));
                body.insert(
                    "anchors".to_owned(),
                    JsonValue::Array(doc.anchors.iter().map(encode_anchor).collect()),
                );
                body.insert(
                    "total_pages".to_owned(),
                    doc.total_pages
                        .map_or(JsonValue::Null, |p| JsonValue::Int(i64::from(p))),
                );
            }
            Self::Observation(obs) => {
                root.insert("kind".to_owned(), JsonValue::Str("observation".to_owned()));
                body.insert("property".to_owned(), JsonValue::Str(obs.property.clone()));
                let mut rv = BTreeMap::new();
                match &obs.recorded_value {
                    RecordedValue::Measured {
                        value,
                        unit,
                        uncertainty,
                    } => {
                        rv.insert("kind".to_owned(), JsonValue::Str("measured".to_owned()));
                        rv.insert("value".to_owned(), JsonValue::finite_float(*value)?);
                        rv.insert("unit".to_owned(), JsonValue::Str(unit.clone()));
                        rv.insert(
                            "uncertainty".to_owned(),
                            match uncertainty {
                                Some(u) => JsonValue::finite_float(*u)?,
                                None => JsonValue::Null,
                            },
                        );
                    }
                    RecordedValue::Estimated {
                        value,
                        unit,
                        method,
                        uncertainty,
                    } => {
                        rv.insert("kind".to_owned(), JsonValue::Str("estimated".to_owned()));
                        rv.insert("value".to_owned(), JsonValue::finite_float(*value)?);
                        rv.insert("unit".to_owned(), JsonValue::Str(unit.clone()));
                        rv.insert("method".to_owned(), JsonValue::Str(method.clone()));
                        rv.insert(
                            "uncertainty".to_owned(),
                            match uncertainty {
                                Some(u) => JsonValue::finite_float(*u)?,
                                None => JsonValue::Null,
                            },
                        );
                    }
                    RecordedValue::Missing { reason } => {
                        rv.insert("kind".to_owned(), JsonValue::Str("missing".to_owned()));
                        rv.insert("reason".to_owned(), JsonValue::Str(reason.clone()));
                    }
                }
                body.insert("recorded_value".to_owned(), JsonValue::Object(rv));
                body.insert(
                    "observer_or_instrument".to_owned(),
                    JsonValue::Str(obs.observer_or_instrument.clone()),
                );
                body.insert(
                    "observed_at_ms".to_owned(),
                    obs.observed_at_ms.map_or(JsonValue::Null, JsonValue::Int),
                );
                body.insert(
                    "original_time_text".to_owned(),
                    JsonValue::Str(obs.original_time_text.clone()),
                );
            }
            Self::Event(ev) => {
                root.insert("kind".to_owned(), JsonValue::Str("event".to_owned()));
                body.insert(
                    "event_kind".to_owned(),
                    JsonValue::Str(ev.event_kind.clone()),
                );
                body.insert("title".to_owned(), JsonValue::Str(ev.title.clone()));
                body.insert(
                    "article_count".to_owned(),
                    JsonValue::Int(ev.article_count as i64),
                );
                body.insert(
                    "account_count".to_owned(),
                    JsonValue::Int(ev.account_count as i64),
                );
                body.insert(
                    "independent_source_count".to_owned(),
                    JsonValue::Int(ev.independent_source_count as i64),
                );
                let mut ch = Vec::with_capacity(ev.cluster_history.len());
                for d in &ev.cluster_history {
                    let mut dm = BTreeMap::new();
                    dm.insert(
                        "decision_id".to_owned(),
                        JsonValue::Str(d.decision_id.as_str().to_owned()),
                    );
                    dm.insert("member_versions".to_owned(), ver_arr(&d.member_versions));
                    dm.insert(
                        "matching_criteria".to_owned(),
                        JsonValue::Str(d.matching_criteria.clone()),
                    );
                    dm.insert(
                        "preserved_conflicts".to_owned(),
                        str_arr(&d.preserved_conflicts),
                    );
                    dm.insert("active".to_owned(), JsonValue::Bool(d.active));
                    dm.insert(
                        "decided_by".to_owned(),
                        JsonValue::Str(d.decided_by.as_str().to_owned()),
                    );
                    ch.push(JsonValue::Object(dm));
                }
                body.insert("cluster_history".to_owned(), JsonValue::Array(ch));
            }
            Self::Claim(cl) => {
                root.insert("kind".to_owned(), JsonValue::Str("claim".to_owned()));
                body.insert(
                    "proposition".to_owned(),
                    JsonValue::Str(cl.proposition.clone()),
                );
                body.insert("claimant".to_owned(), JsonValue::Str(cl.claimant.clone()));
                let mut nat = BTreeMap::new();
                match &cl.nature {
                    ClaimNature::FactualProposition => {
                        nat.insert(
                            "kind".to_owned(),
                            JsonValue::Str("factual_proposition".to_owned()),
                        );
                    }
                    ClaimNature::AttributedStatement { speaker } => {
                        nat.insert(
                            "kind".to_owned(),
                            JsonValue::Str("attributed_statement".to_owned()),
                        );
                        nat.insert("speaker".to_owned(), JsonValue::Str(speaker.clone()));
                    }
                    ClaimNature::AnalyticalProposal => {
                        nat.insert(
                            "kind".to_owned(),
                            JsonValue::Str("analytical_proposal".to_owned()),
                        );
                    }
                }
                body.insert("nature".to_owned(), JsonValue::Object(nat));
                body.insert(
                    "supporting_anchor".to_owned(),
                    match &cl.supporting_anchor {
                        Some(a) => encode_anchor(a),
                        None => JsonValue::Null,
                    },
                );
                body.insert(
                    "assessment".to_owned(),
                    JsonValue::Str(cl.assessment.as_str().to_owned()),
                );
                body.insert(
                    "assessment_rationale".to_owned(),
                    JsonValue::Str(cl.assessment_rationale.clone()),
                );
            }
            Self::Investigation(inv) => {
                root.insert(
                    "kind".to_owned(),
                    JsonValue::Str("investigation".to_owned()),
                );
                body.insert("title".to_owned(), JsonValue::Str(inv.title.clone()));
                body.insert("question".to_owned(), JsonValue::Str(inv.question.clone()));
                body.insert(
                    "geographic_scope".to_owned(),
                    match &inv.geographic_scope {
                        Some(s) => JsonValue::Str(s.clone()),
                        None => JsonValue::Null,
                    },
                );
                body.insert(
                    "temporal_scope".to_owned(),
                    match &inv.temporal_scope {
                        Some(s) => JsonValue::Str(s.clone()),
                        None => JsonValue::Null,
                    },
                );
                body.insert("topics".to_owned(), str_arr(&inv.topics));
                body.insert(
                    "evidence_versions".to_owned(),
                    ver_arr(&inv.evidence_versions),
                );
                body.insert(
                    "saved_query_versions".to_owned(),
                    ver_arr(&inv.saved_query_versions),
                );
                body.insert(
                    "annotation_versions".to_owned(),
                    ver_arr(&inv.annotation_versions),
                );
                body.insert(
                    "hypothesis_versions".to_owned(),
                    ver_arr(&inv.hypothesis_versions),
                );
                body.insert("lead_versions".to_owned(), ver_arr(&inv.lead_versions));
                body.insert("is_published".to_owned(), JsonValue::Bool(inv.is_published));
            }
            Self::Report(rep) => {
                root.insert("kind".to_owned(), JsonValue::Str("report".to_owned()));
                body.insert(
                    "investigation_id".to_owned(),
                    JsonValue::Str(rep.investigation_id.as_str().to_owned()),
                );
                body.insert("title".to_owned(), JsonValue::Str(rep.title.clone()));
                let mut blocks_arr = Vec::with_capacity(rep.blocks.len());
                for b in &rep.blocks {
                    let mut bm = BTreeMap::new();
                    bm.insert(
                        "block_id".to_owned(),
                        JsonValue::Str(b.block_id.as_str().to_owned()),
                    );
                    bm.insert("statement".to_owned(), JsonValue::Str(b.statement.clone()));
                    bm.insert("cited_versions".to_owned(), ver_arr(&b.cited_versions));
                    bm.insert(
                        "citation_anchors".to_owned(),
                        JsonValue::Array(b.citation_anchors.iter().map(encode_anchor).collect()),
                    );
                    bm.insert("ai_generated".to_owned(), JsonValue::Bool(b.ai_generated));
                    bm.insert(
                        "review_status".to_owned(),
                        JsonValue::Str(b.review_status.as_str().to_owned()),
                    );
                    bm.insert(
                        "redact_in_restricted_export".to_owned(),
                        JsonValue::Bool(b.redact_in_restricted_export),
                    );
                    blocks_arr.push(JsonValue::Object(bm));
                }
                body.insert("blocks".to_owned(), JsonValue::Array(blocks_arr));
                body.insert(
                    "limitations_and_open_questions".to_owned(),
                    str_arr(&rep.limitations_and_open_questions),
                );
                body.insert(
                    "publication_state".to_owned(),
                    JsonValue::Str(rep.publication_state.as_str().to_owned()),
                );
            }
            Self::Review(rev) => {
                root.insert("kind".to_owned(), JsonValue::Str("review".to_owned()));
                body.insert(
                    "target_version_id".to_owned(),
                    JsonValue::Str(rev.target_version_id.as_str().to_owned()),
                );
                body.insert(
                    "resulting_status".to_owned(),
                    JsonValue::Str(rev.resulting_status.as_str().to_owned()),
                );
                body.insert(
                    "rationale".to_owned(),
                    JsonValue::Str(rev.rationale.clone()),
                );
                body.insert(
                    "cited_evidence_versions".to_owned(),
                    ver_arr(&rev.cited_evidence_versions),
                );
            }
            Self::Annotation(ann) => {
                root.insert("kind".to_owned(), JsonValue::Str("annotation".to_owned()));
                body.insert(
                    "target_version_id".to_owned(),
                    JsonValue::Str(ann.target_version_id.as_str().to_owned()),
                );
                body.insert("body".to_owned(), JsonValue::Str(ann.body.clone()));
            }
            Self::SavedQuery(sq) => {
                root.insert("kind".to_owned(), JsonValue::Str("saved_query".to_owned()));
                body.insert("name".to_owned(), JsonValue::Str(sq.name.clone()));
                body.insert(
                    "query_text".to_owned(),
                    JsonValue::Str(sq.query_text.clone()),
                );
                body.insert(
                    "case_scope".to_owned(),
                    match &sq.case_scope {
                        Some(c) => JsonValue::Str(c.as_str().to_owned()),
                        None => JsonValue::Null,
                    },
                );
                body.insert("topic_filters".to_owned(), str_arr(&sq.topic_filters));
            }
            Self::Hypothesis(hyp) => {
                root.insert("kind".to_owned(), JsonValue::Str("hypothesis".to_owned()));
                body.insert(
                    "statement".to_owned(),
                    JsonValue::Str(hyp.statement.clone()),
                );
                body.insert(
                    "supporting_versions".to_owned(),
                    ver_arr(&hyp.supporting_versions),
                );
                body.insert(
                    "contradicting_versions".to_owned(),
                    ver_arr(&hyp.contradicting_versions),
                );
                body.insert(
                    "discriminating_observables".to_owned(),
                    str_arr(&hyp.discriminating_observables),
                );
            }
            Self::ResearchLead(rl) => {
                root.insert(
                    "kind".to_owned(),
                    JsonValue::Str("research_lead".to_owned()),
                );
                body.insert("question".to_owned(), JsonValue::Str(rl.question.clone()));
                body.insert(
                    "candidate_sources".to_owned(),
                    str_arr(&rl.candidate_sources),
                );
                body.insert("status".to_owned(), JsonValue::Str(rl.status.clone()));
            }
        }
        root.insert("body".to_owned(), JsonValue::Object(body));
        Ok(JsonValue::Object(root))
    }

    /// Deserializes a [`DomainObject`] from a [`JsonValue`] (`A1`).
    pub fn from_json_value(val: &JsonValue) -> Result<Self, DomainError> {
        let root = val.as_object()?;
        let env_val = root
            .get("envelope")
            .ok_or_else(|| DomainError::Json(JsonError::Schema("missing 'envelope'".to_owned())))?;
        let envelope = ObjectEnvelope::from_json_value(env_val)?;
        let kind = JsonValue::req_str(root, "kind")?;
        let body = JsonValue::req_obj(root, "body")?;

        let obj = match kind {
            "source" => {
                let rc_m = JsonValue::req_obj(body, "reuse_conditions")?;
                Self::Source(Source {
                    envelope,
                    provider_name: JsonValue::req_str(body, "provider_name")?.to_owned(),
                    capabilities: parse_str_vec(body, "capabilities")?,
                    reuse_conditions: ReuseConditions {
                        ordinary_access_method: JsonValue::req_str(rc_m, "ordinary_access_method")?
                            .to_owned(),
                        automated_processing_allowed: JsonValue::req_bool(
                            rc_m,
                            "automated_processing_allowed",
                        )?,
                        retention_allowed: JsonValue::req_bool(rc_m, "retention_allowed")?,
                        redistribution_allowed: JsonValue::req_bool(
                            rc_m,
                            "redistribution_allowed",
                        )?,
                        attribution: JsonValue::req_str(rc_m, "attribution")?.to_owned(),
                    },
                    credential_ref: match JsonValue::opt_str(body, "credential_ref")? {
                        Some(s) => Some(OpaqueId::new(s)?),
                        None => None,
                    },
                    health: SourceHealthState::from_tag(JsonValue::req_str(body, "health")?)?,
                })
            }
            "source_record" => {
                let cm = JsonValue::req_obj(body, "content")?;
                let content = match JsonValue::req_str(cm, "kind")? {
                    "retained_blob" => RetainedContent::RetainedBlob {
                        content_hash: ContentHash::from_hex(JsonValue::req_str(
                            cm,
                            "content_hash",
                        )?)?,
                        byte_len: u64::try_from(JsonValue::req_i64(cm, "byte_len")?).map_err(
                            |_| {
                                DomainError::Json(JsonError::Schema("negative byte_len".to_owned()))
                            },
                        )?,
                        media_type: JsonValue::req_str(cm, "media_type")?.to_owned(),
                    },
                    "omitted" => RetainedContent::Omitted {
                        reason: OmissionReason::from_tag(JsonValue::req_str(cm, "reason")?)?,
                        known_upstream_hash: match JsonValue::opt_str(cm, "known_upstream_hash")? {
                            Some(h) => Some(ContentHash::from_hex(&h)?),
                            None => None,
                        },
                    },
                    other => {
                        return Err(DomainError::InvalidEnumTag {
                            field: "content.kind",
                            value: other.to_owned(),
                        });
                    }
                };
                Self::SourceRecord(SourceRecord {
                    envelope,
                    source_id: OpaqueId::new(JsonValue::req_str(body, "source_id")?)?,
                    upstream_version: JsonValue::req_str(body, "upstream_version")?.to_owned(),
                    acquisition_attempt_id: OpaqueId::new(JsonValue::req_str(
                        body,
                        "acquisition_attempt_id",
                    )?)?,
                    source_locator: JsonValue::req_str(body, "source_locator")?.to_owned(),
                    source_event_time_ms: JsonValue::opt_i64(body, "source_event_time_ms")?,
                    public_availability_time_ms: JsonValue::opt_i64(
                        body,
                        "public_availability_time_ms",
                    )?,
                    retrieved_at_ms: JsonValue::req_i64(body, "retrieved_at_ms")?,
                    content,
                })
            }
            "artifact" => Self::Artifact(Artifact {
                envelope,
                content_hash: ContentHash::from_hex(JsonValue::req_str(body, "content_hash")?)?,
                media_type: JsonValue::req_str(body, "media_type")?.to_owned(),
                byte_len: u64::try_from(JsonValue::req_i64(body, "byte_len")?).map_err(|_| {
                    DomainError::Json(JsonError::Schema("negative byte_len".to_owned()))
                })?,
                derived_from_artifact: match JsonValue::opt_str(body, "derived_from_artifact")? {
                    Some(v) => Some(VersionId::new(v)?),
                    None => None,
                },
                quality_effect_disclosure: JsonValue::opt_str(body, "quality_effect_disclosure")?,
            }),
            "document" => {
                let rk_m = JsonValue::req_obj(body, "release_kind")?;
                let release_kind = match JsonValue::req_str(rk_m, "kind")? {
                    "original_release" => DocumentReleaseKind::OriginalRelease,
                    "revision" => DocumentReleaseKind::Revision {
                        previous_document_version: VersionId::new(JsonValue::req_str(
                            rk_m,
                            "previous_document_version",
                        )?)?,
                    },
                    "differently_redacted_release" => {
                        DocumentReleaseKind::DifferentlyRedactedRelease {
                            related_document_version: VersionId::new(JsonValue::req_str(
                                rk_m,
                                "related_document_version",
                            )?)?,
                            redaction_note: JsonValue::req_str(rk_m, "redaction_note")?.to_owned(),
                        }
                    }
                    "syndicated_or_republished" => DocumentReleaseKind::SyndicatedOrRepublished {
                        upstream_source_version: match JsonValue::opt_str(
                            rk_m,
                            "upstream_source_version",
                        )? {
                            Some(v) => Some(VersionId::new(v)?),
                            None => None,
                        },
                    },
                    other => {
                        return Err(DomainError::InvalidEnumTag {
                            field: "release_kind.kind",
                            value: other.to_owned(),
                        });
                    }
                };
                let pass_obj = JsonValue::req_obj(body, "passages")?;
                let mut passages = BTreeMap::new();
                for (k, v) in pass_obj {
                    let s = match v {
                        JsonValue::Str(s) => s.clone(),
                        _ => {
                            return Err(DomainError::Json(JsonError::Schema(
                                "passage value must be string".to_owned(),
                            )));
                        }
                    };
                    passages.insert(OpaqueId::new(k.clone())?, s);
                }
                let anchors_arr = JsonValue::req_array(body, "anchors")?;
                let mut anchors = Vec::with_capacity(anchors_arr.len());
                for item in anchors_arr {
                    anchors.push(decode_anchor(item)?);
                }
                let total_pages = match JsonValue::opt_i64(body, "total_pages")? {
                    Some(n) => Some(u32::try_from(n).map_err(|_| {
                        DomainError::Json(JsonError::Schema("invalid total_pages".to_owned()))
                    })?),
                    None => None,
                };
                Self::Document(Document {
                    envelope,
                    source_record_version: VersionId::new(JsonValue::req_str(
                        body,
                        "source_record_version",
                    )?)?,
                    original_artifact_version: match JsonValue::opt_str(
                        body,
                        "original_artifact_version",
                    )? {
                        Some(v) => Some(VersionId::new(v)?),
                        None => None,
                    },
                    title: JsonValue::req_str(body, "title")?.to_owned(),
                    authors: parse_str_vec(body, "authors")?,
                    publisher: JsonValue::req_str(body, "publisher")?.to_owned(),
                    language: JsonValue::req_str(body, "language")?.to_owned(),
                    release_kind,
                    passages,
                    anchors,
                    total_pages,
                })
            }
            "observation" => {
                let rv_m = JsonValue::req_obj(body, "recorded_value")?;
                let recorded_value = match JsonValue::req_str(rv_m, "kind")? {
                    "measured" => RecordedValue::Measured {
                        value: JsonValue::req_f64(rv_m, "value")?,
                        unit: JsonValue::req_str(rv_m, "unit")?.to_owned(),
                        uncertainty: JsonValue::opt_f64(rv_m, "uncertainty")?,
                    },
                    "estimated" => RecordedValue::Estimated {
                        value: JsonValue::req_f64(rv_m, "value")?,
                        unit: JsonValue::req_str(rv_m, "unit")?.to_owned(),
                        method: JsonValue::req_str(rv_m, "method")?.to_owned(),
                        uncertainty: JsonValue::opt_f64(rv_m, "uncertainty")?,
                    },
                    "missing" => RecordedValue::Missing {
                        reason: JsonValue::req_str(rv_m, "reason")?.to_owned(),
                    },
                    other => {
                        return Err(DomainError::InvalidEnumTag {
                            field: "recorded_value.kind",
                            value: other.to_owned(),
                        });
                    }
                };
                Self::Observation(Observation {
                    envelope,
                    property: JsonValue::req_str(body, "property")?.to_owned(),
                    recorded_value,
                    observer_or_instrument: JsonValue::req_str(body, "observer_or_instrument")?
                        .to_owned(),
                    observed_at_ms: JsonValue::opt_i64(body, "observed_at_ms")?,
                    original_time_text: JsonValue::req_str(body, "original_time_text")?.to_owned(),
                })
            }
            "event" => {
                let ch_arr = JsonValue::req_array(body, "cluster_history")?;
                let mut cluster_history = Vec::with_capacity(ch_arr.len());
                for item in ch_arr {
                    let dm = item.as_object()?;
                    cluster_history.push(ReversibleClusterDecision {
                        decision_id: OpaqueId::new(JsonValue::req_str(dm, "decision_id")?)?,
                        member_versions: parse_ver_vec(dm, "member_versions")?,
                        matching_criteria: JsonValue::req_str(dm, "matching_criteria")?.to_owned(),
                        preserved_conflicts: parse_str_vec(dm, "preserved_conflicts")?,
                        active: JsonValue::req_bool(dm, "active")?,
                        decided_by: ActorId::new(JsonValue::req_str(dm, "decided_by")?)?,
                    });
                }
                Self::Event(Event {
                    envelope,
                    event_kind: JsonValue::req_str(body, "event_kind")?.to_owned(),
                    title: JsonValue::req_str(body, "title")?.to_owned(),
                    article_count: usize::try_from(JsonValue::req_i64(body, "article_count")?)
                        .unwrap_or(0),
                    account_count: usize::try_from(JsonValue::req_i64(body, "account_count")?)
                        .unwrap_or(0),
                    independent_source_count: usize::try_from(JsonValue::req_i64(
                        body,
                        "independent_source_count",
                    )?)
                    .unwrap_or(0),
                    cluster_history,
                })
            }
            "claim" => {
                let nat_m = JsonValue::req_obj(body, "nature")?;
                let nature = match JsonValue::req_str(nat_m, "kind")? {
                    "factual_proposition" => ClaimNature::FactualProposition,
                    "attributed_statement" => ClaimNature::AttributedStatement {
                        speaker: JsonValue::req_str(nat_m, "speaker")?.to_owned(),
                    },
                    "analytical_proposal" => ClaimNature::AnalyticalProposal,
                    other => {
                        return Err(DomainError::InvalidEnumTag {
                            field: "nature.kind",
                            value: other.to_owned(),
                        });
                    }
                };
                let supporting_anchor = match body.get("supporting_anchor") {
                    None | Some(JsonValue::Null) => None,
                    Some(v) => Some(decode_anchor(v)?),
                };
                Self::Claim(Claim {
                    envelope,
                    proposition: JsonValue::req_str(body, "proposition")?.to_owned(),
                    claimant: JsonValue::req_str(body, "claimant")?.to_owned(),
                    nature,
                    supporting_anchor,
                    assessment: ClaimAssessment::from_tag(JsonValue::req_str(body, "assessment")?)?,
                    assessment_rationale: JsonValue::req_str(body, "assessment_rationale")?
                        .to_owned(),
                })
            }
            "investigation" => Self::Investigation(Investigation {
                envelope,
                title: JsonValue::req_str(body, "title")?.to_owned(),
                question: JsonValue::req_str(body, "question")?.to_owned(),
                geographic_scope: JsonValue::opt_str(body, "geographic_scope")?,
                temporal_scope: JsonValue::opt_str(body, "temporal_scope")?,
                topics: parse_str_vec(body, "topics")?,
                evidence_versions: parse_ver_vec(body, "evidence_versions")?,
                saved_query_versions: parse_ver_vec(body, "saved_query_versions")?,
                annotation_versions: parse_ver_vec(body, "annotation_versions")?,
                hypothesis_versions: parse_ver_vec(body, "hypothesis_versions")?,
                lead_versions: parse_ver_vec(body, "lead_versions")?,
                is_published: JsonValue::req_bool(body, "is_published")?,
            }),
            "report" => {
                let b_arr = JsonValue::req_array(body, "blocks")?;
                let mut blocks = Vec::with_capacity(b_arr.len());
                for item in b_arr {
                    let bm = item.as_object()?;
                    let ca_arr = JsonValue::req_array(bm, "citation_anchors")?;
                    let mut citation_anchors = Vec::with_capacity(ca_arr.len());
                    for a in ca_arr {
                        citation_anchors.push(decode_anchor(a)?);
                    }
                    blocks.push(ReportBlock {
                        block_id: OpaqueId::new(JsonValue::req_str(bm, "block_id")?)?,
                        statement: JsonValue::req_str(bm, "statement")?.to_owned(),
                        cited_versions: parse_ver_vec(bm, "cited_versions")?,
                        citation_anchors,
                        ai_generated: JsonValue::req_bool(bm, "ai_generated")?,
                        review_status: ReviewStatus::from_tag(JsonValue::req_str(
                            bm,
                            "review_status",
                        )?)?,
                        redact_in_restricted_export: JsonValue::req_bool(
                            bm,
                            "redact_in_restricted_export",
                        )?,
                    });
                }
                Self::Report(Report {
                    envelope,
                    investigation_id: OpaqueId::new(JsonValue::req_str(body, "investigation_id")?)?,
                    title: JsonValue::req_str(body, "title")?.to_owned(),
                    blocks,
                    limitations_and_open_questions: parse_str_vec(
                        body,
                        "limitations_and_open_questions",
                    )?,
                    publication_state: ReportPublicationState::from_tag(JsonValue::req_str(
                        body,
                        "publication_state",
                    )?)?,
                })
            }
            "review" => Self::Review(Review {
                envelope,
                target_version_id: VersionId::new(JsonValue::req_str(body, "target_version_id")?)?,
                resulting_status: ReviewStatus::from_tag(JsonValue::req_str(
                    body,
                    "resulting_status",
                )?)?,
                rationale: JsonValue::req_str(body, "rationale")?.to_owned(),
                cited_evidence_versions: parse_ver_vec(body, "cited_evidence_versions")?,
            }),
            "annotation" => Self::Annotation(Annotation {
                envelope,
                target_version_id: VersionId::new(JsonValue::req_str(body, "target_version_id")?)?,
                body: JsonValue::req_str(body, "body")?.to_owned(),
            }),
            "saved_query" => Self::SavedQuery(SavedQuery {
                envelope,
                name: JsonValue::req_str(body, "name")?.to_owned(),
                query_text: JsonValue::req_str(body, "query_text")?.to_owned(),
                case_scope: match JsonValue::opt_str(body, "case_scope")? {
                    Some(s) => Some(OpaqueId::new(s)?),
                    None => None,
                },
                topic_filters: parse_str_vec(body, "topic_filters")?,
            }),
            "hypothesis" => Self::Hypothesis(Hypothesis {
                envelope,
                statement: JsonValue::req_str(body, "statement")?.to_owned(),
                supporting_versions: parse_ver_vec(body, "supporting_versions")?,
                contradicting_versions: parse_ver_vec(body, "contradicting_versions")?,
                discriminating_observables: parse_str_vec(body, "discriminating_observables")?,
            }),
            "research_lead" => Self::ResearchLead(ResearchLead {
                envelope,
                question: JsonValue::req_str(body, "question")?.to_owned(),
                candidate_sources: parse_str_vec(body, "candidate_sources")?,
                status: JsonValue::req_str(body, "status")?.to_owned(),
            }),
            other => {
                return Err(DomainError::InvalidEnumTag {
                    field: "DomainObject.kind",
                    value: other.to_owned(),
                });
            }
        };
        obj.validate()?;
        Ok(obj)
    }
}
