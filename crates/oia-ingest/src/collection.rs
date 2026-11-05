//! Session-only shared source registry. Refreshes are staged by the application before publication.
#[cfg(test)]
mod tests;
use crate::{
    parsing,
    transport::{Request, Response, Transport, public_url},
};
use oia_domain::{
    ActorId, OpaqueId, VersionId,
    evidence::{
        AttributeName, AttributeValue, Availability, EvidenceLedger, FieldValue, LedgerLimits,
        ObjectType, OriginClass, Provenance, RecordFields, ReviewStatus, Scope,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fmt,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

pub const MAX_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_ITEMS: usize = 30;
pub const MAX_SNAPSHOTS: usize = 4;
pub const MAX_SOURCES_PER_CATEGORY: usize = 3;
pub type Check<'a> = &'a dyn Fn() -> Result<(), Error>;

#[derive(Clone, Debug, Serialize)]
pub struct Error {
    pub code: &'static str,
    pub message: String,
}
impl Error {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Error {}
fn malformed(message: impl ToString) -> Error {
    Error::new("malformed", message.to_string())
}

pub fn now_ms() -> Result<i64, Error> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(malformed)?
            .as_millis(),
    )
    .map_err(malformed)
}
pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    News,
    Reports,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Format {
    Feed,
    Archive,
}

#[derive(Clone, Debug, Serialize)]
pub struct UsePolicy {
    pub access: String,
    pub automation: String,
    pub retention: String,
    pub redistribution: String,
    pub reference: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub category: Category,
    pub cadence_seconds: u64,
    pub policy: UsePolicy,
    #[serde(skip)]
    pub(crate) format: Format,
}

#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub record_id: String,
    pub version_id: String,
    pub provider_id: String,
    pub title: String,
    pub url: Option<String>,
    /// XML-decoded feed excerpt or JSON catalogue description; never source HTML execution.
    pub excerpt: Option<String>,
    pub published_time: Option<String>,
    pub updated_time: Option<String>,
    /// A catalogue's covering dates, never assumed to be the sighting/event time.
    pub archive_covering_dates: Option<String>,
    pub archive_reference: Option<String>,
    pub response_sha256: String,
    pub item_sha256: String,
    pub byte_start: usize,
    pub byte_end: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct SnapshotInfo {
    pub sha256: String,
    pub retrieved_at_ms: i64,
    pub request_url: String,
    pub bytes: usize,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub supplied_items: usize,
    pub excluded_items: usize,
    pub coverage_note: String,
}

#[derive(Debug)]
pub struct Snapshot {
    pub info: SnapshotInfo,
    pub entries: Vec<Entry>,
    original: Vec<u8>,
    ledger: EvidenceLedger,
}
impl Snapshot {
    #[must_use]
    pub fn original(&self) -> &[u8] {
        &self.original
    }
    #[must_use]
    pub fn evidence(&self) -> &EvidenceLedger {
        &self.ledger
    }
}

#[derive(Clone, Debug)]
struct State {
    source: Source,
    outcome: String,
    message: String,
    last_attempt_ms: Option<i64>,
    last_success_ms: Option<i64>,
    next_attempt_ms: i64,
    current: Option<Arc<Snapshot>>,
    history: Vec<Arc<Snapshot>>,
}
impl State {
    fn new(source: Source) -> Self {
        Self {
            source,
            outcome: "not_collected".into(),
            message: "Not yet queried.".into(),
            last_attempt_ms: None,
            last_success_ms: None,
            next_attempt_ms: 0,
            current: None,
            history: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SourceView {
    pub source: Source,
    pub outcome: String,
    pub message: String,
    pub last_attempt_ms: Option<i64>,
    pub last_success_ms: Option<i64>,
    pub next_attempt_ms: i64,
    pub stale: bool,
    pub current: Option<SnapshotInfo>,
    pub snapshots: Vec<SnapshotInfo>,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Serialize)]
pub struct View {
    pub category: Category,
    pub sources: Vec<SourceView>,
    pub max_response_bytes: usize,
    pub max_items_per_source: usize,
    pub max_snapshots_per_source: usize,
    pub storage: &'static str,
    pub origin: &'static str,
    pub review: &'static str,
}

#[derive(Clone, Debug)]
pub struct Registry {
    sources: Vec<State>,
}
impl Default for Registry {
    fn default() -> Self {
        let retention = "Bounded session memory only; originals and extractions discarded on explicit clear or exit. No disk cache.";
        let seeds = [
            (
                "nasa-news",
                "NASA",
                "https://www.nasa.gov/feed/",
                Category::News,
                Format::Feed,
                3600,
                "Publisher-provided RSS for informational reading; at most hourly.",
                "NASA material generally permits informational use; third-party credits and protected marks remain separate. No payload export in this workflow.",
                "https://www.nasa.gov/nasa-brand-center/images-and-media/",
            ),
            (
                "bbc-world",
                "BBC News · World",
                "https://feeds.bbci.co.uk/news/world/rss.xml",
                Category::News,
                Format::Feed,
                900,
                "Publisher-provided news feed for personal, non-commercial reading; at most every 15 minutes.",
                "BBC copyright; feed reading only. Full articles are not fetched. No republication, model processing or payload export in this workflow.",
                "https://www.bbc.co.uk/usingthebbc/terms-of-use/#15metadataandrssfeeds",
            ),
            (
                "tna-ufo",
                "The National Archives · UFO reports",
                "https://discovery.nationalarchives.gov.uk/API/search/v1/records",
                Category::Reports,
                Format::Archive,
                10,
                "Documented public Discovery search API; one first-page query, at most every 10 seconds; automatic refresh every 15 minutes.",
                "Catalogue metadata under the published reuse terms; document/image rights require separate assessment. Original testimony and document scans are not acquired or exported.",
                "https://www.nationalarchives.gov.uk/legal/terms-of-use/",
            ),
        ];
        Self { sources: seeds.into_iter().map(|(id, name, endpoint, category, format, cadence_seconds, automation, redistribution, reference)| State::new(Source {
            id: id.into(), name: name.into(), endpoint: endpoint.into(), category, format, cadence_seconds,
            policy: UsePolicy { access: "Public HTTPS GET without credentials; restrictions stop collection.".into(), automation: automation.into(), retention: retention.into(), redistribution: redistribution.into(), reference: reference.into() },
        })).collect() }
    }
}

impl Registry {
    /// Explicitly registers a public user-selected feed, rather than treating seed hosts as a
    /// permanent allow-list. Reuse rights stay unknown; the operation is personal feed reading.
    pub fn add_feed(
        &mut self,
        name: String,
        endpoint: String,
        category: Category,
    ) -> Result<(), Error> {
        if name.trim().is_empty() || name.len() > 160 || name.chars().any(char::is_control) {
            return Err(Error::new(
                "invalid_source",
                "Use a source name of 1–160 bytes without control characters.",
            ));
        }
        let endpoint = public_url(endpoint.trim())?.to_string();
        if self.sources.iter().any(|s| s.source.endpoint == endpoint) {
            return Err(Error::new(
                "duplicate_source",
                "That source is already registered.",
            ));
        }
        if self
            .sources
            .iter()
            .filter(|s| s.source.category == category)
            .count()
            >= MAX_SOURCES_PER_CATEGORY
        {
            return Err(Error::new(
                "source_limit",
                "This session supports at most three sources in each instrument.",
            ));
        }
        let id = format!("feed-{}", &hash(endpoint.as_bytes())[..24]);
        self.sources.push(State::new(Source { id, name, endpoint: endpoint.clone(), category, format: Format::Feed, cadence_seconds: 900,
            policy: UsePolicy { access: "User-selected public HTTPS RSS/Atom feed, without credentials.".into(), automation: "Personal feed reading; at most every 15 minutes. Source restrictions stop requests; no article crawling or model processing.".into(), retention: "Bounded session memory, discarded on clear or exit; no durable retention.".into(), redistribution: "Unknown; no payload export or redistribution is supported.".into(), reference: endpoint },
        }));
        Ok(())
    }

    pub fn view(&self, category: Category, now: i64) -> View {
        View {
            category,
            max_response_bytes: MAX_BYTES,
            max_items_per_source: MAX_ITEMS,
            max_snapshots_per_source: MAX_SNAPSHOTS,
            storage: "session_memory_only",
            origin: OriginClass::AcquiredSourceMaterial.as_str(),
            review: ReviewStatus::Unreviewed.as_str(),
            sources: self
                .sources
                .iter()
                .filter(|s| s.source.category == category)
                .map(|s| SourceView {
                    source: s.source.clone(),
                    outcome: s.outcome.clone(),
                    message: s.message.clone(),
                    last_attempt_ms: s.last_attempt_ms,
                    last_success_ms: s.last_success_ms,
                    next_attempt_ms: s.next_attempt_ms,
                    stale: s.current.is_some()
                        && (s.outcome != "ready" && s.outcome != "empty"
                            || s.last_success_ms.is_none_or(|time| {
                                now.saturating_sub(time) >= (s.source.cadence_seconds * 1000) as i64
                            })),
                    current: s.current.as_ref().map(|snapshot| snapshot.info.clone()),
                    snapshots: s
                        .history
                        .iter()
                        .map(|snapshot| snapshot.info.clone())
                        .collect(),
                    entries: s
                        .current
                        .as_ref()
                        .map(|snapshot| snapshot.entries.clone())
                        .unwrap_or_default(),
                })
                .collect(),
        }
    }

    pub fn clear(&mut self, category: Category) {
        for state in self
            .sources
            .iter_mut()
            .filter(|s| s.source.category == category)
        {
            // Clearing bytes never bypasses the source cadence or a rate-limit backoff.
            state.current = None;
            state.history.clear();
            state.outcome = "not_collected".into();
            state.message =
                "Retained responses cleared; source cadence/backoff is still in effect.".into();
            state.last_success_ms = None;
        }
    }

    pub fn snapshot(
        &self,
        source_id: &str,
        sha256: &str,
        request_url: &str,
    ) -> Result<Arc<Snapshot>, Error> {
        self.sources
            .iter()
            .find(|s| s.source.id == source_id)
            .and_then(|s| {
                s.history.iter().find(|snapshot| {
                    snapshot.info.sha256 == sha256 && snapshot.info.request_url == request_url
                })
            })
            .cloned()
            .ok_or_else(|| {
                Error::new(
                    "stale_input",
                    "That retained source response is no longer held.",
                )
            })
    }

    pub fn refresh(
        &mut self,
        category: Category,
        query: &str,
        transport: &mut impl Transport,
        check: Check<'_>,
    ) -> Result<(), Error> {
        if query.len() > 120 || query.chars().any(char::is_control) {
            return Err(Error::new(
                "invalid_query",
                "Archive search accepts at most 120 UTF-8 bytes without control characters.",
            ));
        }
        for state in self
            .sources
            .iter_mut()
            .filter(|s| s.source.category == category)
        {
            check()?;
            let now = now_ms()?;
            if now < state.next_attempt_ms {
                continue;
            }
            let mut url = public_url(&state.source.endpoint)?;
            if state.source.format == Format::Archive {
                let search = if query.trim().is_empty() {
                    "UFO sighting".into()
                } else {
                    format!("UFO {}", query.trim())
                };
                url.query_pairs_mut()
                    .append_pair("sps.searchQuery", &search)
                    .append_pair("sps.resultsPageSize", "30");
            }
            state.last_attempt_ms = Some(now);
            state.next_attempt_ms =
                now.saturating_add((state.source.cadence_seconds * 1000) as i64);
            let validators = state
                .current
                .as_ref()
                .filter(|snapshot| snapshot.info.request_url == url.as_str());
            let request = Request {
                url: url.as_str(),
                cadence_seconds: state.source.cadence_seconds,
                etag: validators.and_then(|snapshot| snapshot.info.etag.as_deref()),
                last_modified: validators
                    .and_then(|snapshot| snapshot.info.last_modified.as_deref()),
            };
            let result = transport.get(request, check).and_then(|response| {
                let delay = response.cache_max_age.unwrap_or(0).max(response.retry_after.unwrap_or(0)).max(state.source.cadence_seconds);
                state.next_attempt_ms = now.saturating_add(i64::try_from(delay.saturating_mul(1000)).unwrap_or(i64::MAX));
                if response.no_store {
                    return Err(Error::new("reference_only", "The source specifies Cache-Control: no-store. This response was not retained; only its endpoint reference is available."));
                }
                if response.status == 304 {
                    let held = state.current.as_ref().filter(|snapshot| snapshot.info.request_url == url.as_str())
                        .ok_or_else(|| Error::new("malformed", "The source returned 304 without a matching retained response."))?;
                    return Ok(held.clone());
                }
                if response.status != 200 {
                    let (code, message) = match response.status {
                        401 => ("unauthorized", "The source requires authorization; no alternate access attempted."),
                        403 => ("restricted", "The source restricted access; no bypass attempted."),
                        429 => ("rate_limited", "The source rate-limited this request; prior results are retained."),
                        300..=399 => ("redirect_blocked", "The source redirected. Redirects are not followed; register the final public feed explicitly."),
                        404 | 410 => ("missing", "The source endpoint is missing or removed; this is not a retraction."),
                        _ => ("unavailable", "The source is unavailable; prior results are retained."),
                    };
                    if response.status == 429 || response.status == 503 {
                        // Unparseable/date-form Retry-After conservatively backs off for 24h.
                        let seconds = response.retry_after.unwrap_or(86400).max(state.source.cadence_seconds);
                        state.next_attempt_ms = now.saturating_add(i64::try_from(seconds.saturating_mul(1000)).unwrap_or(i64::MAX));
                    }
                    return Err(Error::new(code, message));
                }
                parse_response(&state.source, url.as_str(), response, now_ms()?, check).map(Arc::new)
            });
            match result {
                Ok(snapshot) => {
                    check()?;
                    let snapshot = if let Some(held) = state.history.iter().find(|s| {
                        s.info.sha256 == snapshot.info.sha256
                            && s.info.request_url == snapshot.info.request_url
                    }) {
                        held.clone()
                    } else if state.history.len() >= MAX_SNAPSHOTS {
                        state.outcome = "cache_full".into();
                        state.message = "Four distinct responses are retained. Clear this instrument's retained responses before acquiring new versions; earlier originals were preserved.".into();
                        continue;
                    } else {
                        state.history.push(snapshot.clone());
                        snapshot
                    };
                    state.outcome = if snapshot.entries.is_empty() {
                        "empty"
                    } else {
                        "ready"
                    }
                    .into();
                    state.message = snapshot.info.coverage_note.clone();
                    state.current = Some(snapshot);
                    state.last_success_ms = Some(now_ms()?);
                }
                Err(error) if error.code == "cancelled" || error.code == "deadline" => {
                    return Err(error);
                }
                Err(error) => {
                    state.outcome = error.code.into();
                    state.message = error.message;
                }
            }
        }
        check()
    }
}

pub(crate) struct Parsed {
    pub entries: Vec<Entry>,
    pub supplied: usize,
    pub excluded: usize,
    pub note: String,
}

fn parse_response(
    source: &Source,
    url: &str,
    response: Response,
    retrieved_at_ms: i64,
    check: Check<'_>,
) -> Result<Snapshot, Error> {
    check()?;
    if response.body.len() > MAX_BYTES {
        return Err(Error::new("byte_limit", "Response exceeds 2 MiB."));
    }
    let digest = hash(&response.body);
    let mut parsed = match source.format {
        Format::Feed => parsing::feed(&source.id, url, &response.body, &digest, check)?,
        Format::Archive => parsing::archive(&source.id, &response.body, &digest, check)?,
    };
    let acquisition = hash(format!("{}\n{url}\n{digest}\n{retrieved_at_ms}", source.id).as_bytes());
    for entry in &mut parsed.entries {
        entry.version_id = format!(
            "itemv-{}",
            hash(format!("{}\n{}\n{acquisition}", entry.record_id, entry.item_sha256).as_bytes())
        );
    }
    let ledger = evidence(source, &digest, url, retrieved_at_ms, &parsed.entries)?;
    check()?;
    Ok(Snapshot {
        info: SnapshotInfo {
            sha256: digest,
            retrieved_at_ms,
            request_url: url.into(),
            bytes: response.body.len(),
            etag: response.etag,
            last_modified: response.last_modified,
            supplied_items: parsed.supplied,
            excluded_items: parsed.excluded,
            coverage_note: parsed.note,
        },
        entries: parsed.entries,
        original: response.body,
        ledger,
    })
}

fn evidence(
    source: &Source,
    digest: &str,
    url: &str,
    now: i64,
    entries: &[Entry],
) -> Result<EvidenceLedger, Error> {
    let mut ledger = EvidenceLedger::new(LedgerLimits::default());
    let source_id = OpaqueId::new(source.id.clone()).map_err(malformed)?;
    let acquisition = hash(format!("{}\n{url}\n{digest}\n{now}", source.id).as_bytes());
    let response_version = VersionId::new(format!("response-{acquisition}")).map_err(malformed)?;
    let mut attributes = BTreeMap::new();
    for (key, value) in [
        ("request_url", url),
        ("sha256", digest),
        ("attribution", &source.name),
        ("retention", &source.policy.retention),
        ("redistribution", &source.policy.redistribution),
    ] {
        attributes.insert(
            AttributeName::new(key).map_err(malformed)?,
            FieldValue::Known(AttributeValue::Text(value.into())),
        );
    }
    ledger
        .issue(RecordFields {
            id: OpaqueId::new(format!(
                "response-{}",
                hash(format!("{}\n{url}", source.id).as_bytes())
            ))
            .map_err(malformed)?,
            version: response_version.clone(),
            object_type: ObjectType::SourceRecord,
            origin: OriginClass::AcquiredSourceMaterial,
            created_at_ms: now,
            created_by: ActorId::new("oia-public-collection").map_err(malformed)?,
            scope: Scope::Library,
            access_policies: vec![OpaqueId::new("session-source-policy-v1").map_err(malformed)?],
            provenance: Provenance::Acquired { source: source_id },
            review: ReviewStatus::Unreviewed,
            availability: Availability::Available,
            attributes,
            claim: None,
        })
        .map_err(malformed)?;
    for entry in entries {
        let attributes = [
            ("title", entry.title.as_str()),
            ("response_sha256", digest),
            ("item_sha256", &entry.item_sha256),
        ]
        .into_iter()
        .map(|(k, v)| {
            Ok((
                AttributeName::new(k).map_err(malformed)?,
                FieldValue::Known(AttributeValue::Text(v.into())),
            ))
        })
        .collect::<Result<_, Error>>()?;
        ledger
            .issue(RecordFields {
                id: OpaqueId::new(entry.record_id.clone()).map_err(malformed)?,
                version: VersionId::new(entry.version_id.clone()).map_err(malformed)?,
                object_type: ObjectType::Document,
                origin: OriginClass::ConventionalDerivative,
                created_at_ms: now,
                created_by: ActorId::new("oia-public-collection").map_err(malformed)?,
                scope: Scope::Library,
                access_policies: vec![
                    OpaqueId::new("session-source-policy-v1").map_err(malformed)?,
                ],
                provenance: Provenance::Derived {
                    method: OpaqueId::new("public-metadata-extraction").map_err(malformed)?,
                    method_version: VersionId::new("public-metadata-extraction-v1")
                        .map_err(malformed)?,
                    inputs: vec![response_version.clone()],
                },
                review: ReviewStatus::Unreviewed,
                availability: Availability::Available,
                attributes,
                claim: None,
            })
            .map_err(malformed)?;
    }
    Ok(ledger)
}
