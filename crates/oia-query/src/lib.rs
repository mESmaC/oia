//! `oia-query` — Permission-aware exact and semantic search, workspace isolation,
//! pagination, and coverage reporting (`CASE-05`, `COLLAB-01`, `A01`, `A16`).

use std::collections::BTreeMap;

use oia_domain::{ActorId, DomainObject, OpaqueId, VersionId, WorkspaceVisibility};
use oia_store::EvidenceStore;

/// Collaborator role within a case (`COLLAB-01`, `docs/architecture.md` Section 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WorkspaceRole {
    /// Full control including sharing, deletion, and sensitive artifact access.
    Owner,
    /// Can add/edit case material and access sensitive case evidence.
    Editor,
    /// Can inspect permitted evidence and submit `Review` assessments; no sensitive access by default.
    Reviewer,
    /// Read-only access to non-sensitive case evidence.
    Viewer,
}

impl WorkspaceRole {
    /// Returns `true` if this role may mutate case evidence or annotations.
    #[must_use]
    pub const fn can_edit_case(self) -> bool {
        matches!(self, Self::Owner | Self::Editor)
    }

    /// Returns `true` if this role may submit `Review` records.
    #[must_use]
    pub const fn can_submit_review(self) -> bool {
        matches!(self, Self::Owner | Self::Editor | Self::Reviewer)
    }

    /// Returns `true` if this role may publish reports or delete case artifacts.
    #[must_use]
    pub const fn can_publish_or_delete(self) -> bool {
        matches!(self, Self::Owner)
    }

    /// Returns `true` if this role may read sensitive-flagged artifacts within the case (`A16`).
    #[must_use]
    pub const fn can_read_sensitive(self) -> bool {
        matches!(self, Self::Owner | Self::Editor)
    }
}

/// Authorization context for a caller executing queries, blob reads, or commands.
#[derive(Debug, Clone, Default)]
pub struct AuthorizationContext {
    /// Actor ID.
    pub actor_id: Option<ActorId>,
    /// Granted role per `case_id`.
    pub roles_by_case: BTreeMap<OpaqueId, WorkspaceRole>,
}

impl AuthorizationContext {
    /// Creates an authorization context for `actor_id`.
    #[must_use]
    pub fn new(actor_id: ActorId) -> Self {
        Self {
            actor_id: Some(actor_id),
            roles_by_case: BTreeMap::new(),
        }
    }

    /// Grants `role` on `case_id`.
    pub fn grant_case_role(&mut self, case_id: OpaqueId, role: WorkspaceRole) {
        self.roles_by_case.insert(case_id, role);
    }

    /// Checks whether this caller is authorized to read an object with `visibility` (`A16`).
    #[must_use]
    pub fn can_read(&self, visibility: &WorkspaceVisibility) -> bool {
        match visibility {
            WorkspaceVisibility::LibraryPublic => true,
            WorkspaceVisibility::PrivateCase { case_id, sensitive }
            | WorkspaceVisibility::SharedCase {
                case_id, sensitive, ..
            } => {
                let Some(&role) = self.roles_by_case.get(case_id) else {
                    return false;
                };
                if *sensitive {
                    role.can_read_sensitive()
                } else {
                    true
                }
            }
        }
    }

    /// Returns the caller's role in `case_id`, if any.
    #[must_use]
    pub fn role_in_case(&self, case_id: &OpaqueId) -> Option<WorkspaceRole> {
        self.roles_by_case.get(case_id).copied()
    }
}

/// Search request combining exact identifier/text filtering with pagination and case scope (`CASE-05`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    /// Exact or substring query text (matched case-insensitively against IDs, titles, propositions, and passages).
    pub query: String,
    /// Optional restriction to a specific investigation `case_id`.
    pub case_scope: Option<OpaqueId>,
    /// Maximum hits to return (`1..=500`).
    pub limit: usize,
    /// 0-based pagination offset.
    pub offset: usize,
}

/// A permission-filtered search hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    /// Matched object ID.
    pub id: OpaqueId,
    /// Matched immutable version ID.
    pub version_id: VersionId,
    /// Matched snippet or summary.
    pub snippet: String,
}

/// Search result with explicit scope, pagination, and coverage limits (`CASE-05`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResponse {
    /// Returned hits for the requested page.
    pub hits: Vec<SearchHit>,
    /// Total permitted matches before pagination slicing.
    pub total_permitted_matches: usize,
    /// Whether additional matches were truncated by `limit`.
    pub truncated_by_pagination: bool,
    /// Document versions in scope that had unreadable/partial pages (`DOC-02`, `A03`).
    pub partial_extraction_document_versions: Vec<VersionId>,
}

/// Executes a permission-filtered search over `store` (`CASE-05`, `A01`, `A16`).
///
/// Unauthorized or cross-workspace private objects are filtered BEFORE matching or snippet
/// extraction so search results never reveal the existence or content of restricted material.
#[must_use]
pub fn execute_scoped_search(
    store: &EvidenceStore,
    auth: &AuthorizationContext,
    req: &SearchRequest,
) -> SearchResponse {
    let needle = req.query.trim().to_ascii_lowercase();
    let limit = req.limit.clamp(1, 500);
    let mut matched = Vec::new();
    let mut partial_docs = Vec::new();

    for (ver_id, obj) in store.graph().all_versions() {
        let env = obj.envelope();
        if !auth.can_read(&env.visibility) {
            continue;
        }
        if let Some(ref required_case) = req.case_scope
            && env.visibility.case_id() != Some(required_case)
        {
            continue;
        }

        if let DomainObject::Document(doc) = obj {
            let has_unreadable = doc
                .anchors
                .iter()
                .any(|a| !matches!(a.coverage, oia_domain::ExtractionCoverage::Complete));
            if has_unreadable {
                partial_docs.push(ver_id.clone());
            }
        }

        let candidate_text = extract_searchable_text(obj);
        if needle.is_empty()
            || env.id.as_str().to_ascii_lowercase().contains(&needle)
            || ver_id.as_str().to_ascii_lowercase().contains(&needle)
            || candidate_text.to_ascii_lowercase().contains(&needle)
        {
            matched.push(SearchHit {
                id: env.id.clone(),
                version_id: ver_id.clone(),
                snippet: candidate_text.chars().take(160).collect(),
            });
        }
    }

    let total = matched.len();
    let page: Vec<SearchHit> = matched.into_iter().skip(req.offset).take(limit).collect();
    let truncated = req.offset.saturating_add(page.len()) < total;

    SearchResponse {
        hits: page,
        total_permitted_matches: total,
        truncated_by_pagination: truncated,
        partial_extraction_document_versions: partial_docs,
    }
}

fn extract_searchable_text(obj: &DomainObject) -> String {
    match obj {
        DomainObject::Source(s) => {
            format!("{} {}", s.provider_name, s.reuse_conditions.attribution)
        }
        DomainObject::SourceRecord(sr) => format!("{} {}", sr.source_locator, sr.upstream_version),
        DomainObject::Artifact(a) => format!("{} {}", a.media_type, a.content_hash),
        DomainObject::Document(d) => {
            let mut text = format!("{} {} ", d.title, d.publisher);
            for p in d.passages.values() {
                text.push_str(p);
                text.push(' ');
            }
            text
        }
        DomainObject::Observation(o) => format!("{} {}", o.property, o.observer_or_instrument),
        DomainObject::Event(e) => format!("{} {}", e.event_kind, e.title),
        DomainObject::Claim(c) => format!("{} {}", c.proposition, c.claimant),
        DomainObject::Investigation(i) => format!("{} {}", i.title, i.question),
        DomainObject::Report(r) => {
            let mut text = r.title.clone();
            for b in &r.blocks {
                text.push(' ');
                text.push_str(&b.statement);
            }
            text
        }
        DomainObject::Review(rev) => rev.rationale.clone(),
        DomainObject::Annotation(ann) => ann.body.clone(),
        DomainObject::SavedQuery(sq) => format!("{} {}", sq.name, sq.query_text),
        DomainObject::Hypothesis(h) => h.statement.clone(),
        DomainObject::ResearchLead(l) => l.question.clone(),
    }
}
