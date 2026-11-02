//! `oia-store` — Transactional evidence storage, journaled disk persistence (`store.json`,
//! `store.wal`, `schema_migrations.json`), content-addressed SHA-256 blobs with `sync_all`
//! durability and staged cleanup, schema migrations, reference-aware retention, tombstones,
//! backup/restore, and durable jobs with lease-guarded completion (`#11`, `#14`, `A1`–`A4`, `A01`, `A17`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use oia_domain::{
    ActorId, Artifact, AvailabilityState, ContentHash, DomainError, DomainObject, JsonBudget,
    JsonError, JsonValue, OpaqueId, RelationEdge, SourceRecord, VersionId,
};

/// Current supported persistence schema version.
pub const CURRENT_STORE_SCHEMA_VERSION: u32 = 2;

/// Retention classification for stored blobs and records (`docs/architecture.md` Section 5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RetentionClass {
    /// Transient live ring buffer eligible for expiry when unpinned.
    RoutineBuffer {
        /// Expiry timestamp (UTC ms).
        expires_at_ms: i64,
    },
    /// Temporary extraction or cache artifact eligible for eviction when unpinned.
    TemporaryExtractionCache {
        /// Expiry timestamp (UTC ms).
        expires_at_ms: i64,
    },
    /// Pinned case evidence retained until explicit deletion/tombstone.
    PinnedCaseEvidence,
    /// Snapshot pinned by an issued report version.
    IssuedReportSnapshot,
}

impl RetentionClass {
    /// Returns `true` if this retention class protects the blob from automatic expiry GC.
    #[must_use]
    pub const fn is_protected(self) -> bool {
        matches!(self, Self::PinnedCaseEvidence | Self::IssuedReportSnapshot)
    }

    /// Returns `true` if expired at `now_ms`.
    #[must_use]
    pub const fn is_expired_at(self, now_ms: i64) -> bool {
        match self {
            Self::RoutineBuffer { expires_at_ms }
            | Self::TemporaryExtractionCache { expires_at_ms } => now_ms >= expires_at_ms,
            Self::PinnedCaseEvidence | Self::IssuedReportSnapshot => false,
        }
    }

    fn to_json_value(self) -> JsonValue {
        let mut m = BTreeMap::new();
        match self {
            Self::RoutineBuffer { expires_at_ms } => {
                m.insert(
                    "kind".to_owned(),
                    JsonValue::Str("routine_buffer".to_owned()),
                );
                m.insert("expires_at_ms".to_owned(), JsonValue::Int(expires_at_ms));
            }
            Self::TemporaryExtractionCache { expires_at_ms } => {
                m.insert(
                    "kind".to_owned(),
                    JsonValue::Str("temporary_extraction_cache".to_owned()),
                );
                m.insert("expires_at_ms".to_owned(), JsonValue::Int(expires_at_ms));
            }
            Self::PinnedCaseEvidence => {
                m.insert(
                    "kind".to_owned(),
                    JsonValue::Str("pinned_case_evidence".to_owned()),
                );
            }
            Self::IssuedReportSnapshot => {
                m.insert(
                    "kind".to_owned(),
                    JsonValue::Str("issued_report_snapshot".to_owned()),
                );
            }
        }
        JsonValue::Object(m)
    }

    fn from_json_value(val: &JsonValue) -> Result<Self, StoreError> {
        let m = val
            .as_object()
            .map_err(|e| StoreError::Domain(DomainError::Json(e)))?;
        let kind =
            JsonValue::req_str(m, "kind").map_err(|e| StoreError::Domain(DomainError::Json(e)))?;
        match kind {
            "routine_buffer" => Ok(Self::RoutineBuffer {
                expires_at_ms: JsonValue::req_i64(m, "expires_at_ms")
                    .map_err(|e| StoreError::Domain(DomainError::Json(e)))?,
            }),
            "temporary_extraction_cache" => Ok(Self::TemporaryExtractionCache {
                expires_at_ms: JsonValue::req_i64(m, "expires_at_ms")
                    .map_err(|e| StoreError::Domain(DomainError::Json(e)))?,
            }),
            "pinned_case_evidence" => Ok(Self::PinnedCaseEvidence),
            "issued_report_snapshot" => Ok(Self::IssuedReportSnapshot),
            other => Err(StoreError::Domain(DomainError::InvalidEnumTag {
                field: "RetentionClass.kind",
                value: other.to_owned(),
            })),
        }
    }
}

/// Immutable tombstone recorded when retained bytes are deleted for license compliance or user action (`docs/architecture.md` Section 5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TombstoneRecord {
    /// Deleted artifact version ID.
    pub artifact_version_id: VersionId,
    /// Content hash of the removed bytes.
    pub content_hash: ContentHash,
    /// UTC timestamp (ms) of deletion.
    pub deleted_at_ms: i64,
    /// Actor who performed the deletion.
    pub deleted_by: ActorId,
    /// Documented reason for deletion.
    pub reason: String,
}

impl TombstoneRecord {
    fn to_json_value(&self) -> JsonValue {
        let mut m = BTreeMap::new();
        m.insert(
            "artifact_version_id".to_owned(),
            JsonValue::Str(self.artifact_version_id.as_str().to_owned()),
        );
        m.insert(
            "content_hash".to_owned(),
            JsonValue::Str(self.content_hash.to_hex()),
        );
        m.insert(
            "deleted_at_ms".to_owned(),
            JsonValue::Int(self.deleted_at_ms),
        );
        m.insert(
            "deleted_by".to_owned(),
            JsonValue::Str(self.deleted_by.as_str().to_owned()),
        );
        m.insert("reason".to_owned(), JsonValue::Str(self.reason.clone()));
        JsonValue::Object(m)
    }

    fn from_json_value(val: &JsonValue) -> Result<Self, StoreError> {
        let m = val
            .as_object()
            .map_err(|e| StoreError::Domain(DomainError::Json(e)))?;
        Ok(Self {
            artifact_version_id: VersionId::new(
                JsonValue::req_str(m, "artifact_version_id")
                    .map_err(|e| StoreError::Domain(DomainError::Json(e)))?,
            )
            .map_err(|e| StoreError::Domain(DomainError::Id(e)))?,
            content_hash: ContentHash::from_hex(
                JsonValue::req_str(m, "content_hash")
                    .map_err(|e| StoreError::Domain(DomainError::Json(e)))?,
            )
            .map_err(|e| StoreError::Domain(DomainError::Hash(e)))?,
            deleted_at_ms: JsonValue::req_i64(m, "deleted_at_ms")
                .map_err(|e| StoreError::Domain(DomainError::Json(e)))?,
            deleted_by: ActorId::new(
                JsonValue::req_str(m, "deleted_by")
                    .map_err(|e| StoreError::Domain(DomainError::Json(e)))?,
            )
            .map_err(|e| StoreError::Domain(DomainError::Id(e)))?,
            reason: JsonValue::req_str(m, "reason")
                .map_err(|e| StoreError::Domain(DomainError::Json(e)))?
                .to_owned(),
        })
    }
}

/// Durable job lifecycle state (`docs/architecture.md` Section 6.1, `CASE-04`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    /// Waiting for an available worker.
    Queued,
    /// Actively leased by a worker.
    Running,
    /// Completed with full coverage.
    Succeeded,
    /// Completed with documented partial coverage/limitations (`CASE-04`).
    PartiallySucceeded,
    /// Terminated with a failure diagnostic.
    Failed,
    /// Explicitly cancelled by user or deadline; late completion is rejected.
    Cancelled,
    /// Superseded by a newer query or job; late completion is rejected.
    Superseded,
}

impl JobState {
    /// Returns `true` if this state is terminal and must reject late worker completions.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded
                | Self::PartiallySucceeded
                | Self::Failed
                | Self::Cancelled
                | Self::Superseded
        )
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::PartiallySucceeded => "partially_succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Superseded => "superseded",
        }
    }

    fn from_tag(tag: &str) -> Result<Self, StoreError> {
        match tag {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "partially_succeeded" => Ok(Self::PartiallySucceeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "superseded" => Ok(Self::Superseded),
            other => Err(StoreError::Domain(DomainError::InvalidEnumTag {
                field: "JobState",
                value: other.to_owned(),
            })),
        }
    }
}

/// Active execution lease assigned to a worker running a [`DurableJob`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobLease {
    /// Monotonically increasing epoch token; revoked on cancellation, supersession, or expiry.
    pub lease_epoch: u64,
    /// Worker identifier.
    pub worker_id: String,
    /// Lease expiration timestamp (UTC ms).
    pub expires_at_ms: i64,
}

/// Durable background job record (`docs/architecture.md` Section 6.1, `#14`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableJob {
    /// Unique job ID.
    pub job_id: OpaqueId,
    /// Owning workspace/case ID, if scoped.
    pub case_id: Option<OpaqueId>,
    /// Operation kind (e.g., `"source_ingest"`, `"document_ocr"`, `"analysis_run"`).
    pub kind: String,
    /// Pinned input version IDs.
    pub input_versions: Vec<VersionId>,
    /// Current lifecycle state.
    pub state: JobState,
    /// Active lease when `state == JobState::Running`.
    pub active_lease: Option<JobLease>,
    /// Monotonically increasing epoch counter used to mint leases and invalidate stale workers.
    pub latest_epoch: u64,
    /// Number of execution attempts started so far.
    pub attempt_count: u32,
    /// Maximum allowed attempts before permanent failure on crash/timeout.
    pub max_attempts: u32,
    /// Progress percentage (`0..=100`).
    pub progress_percent: u8,
    /// Output version IDs published atomically on completion.
    pub output_versions: Vec<VersionId>,
    /// Partial-coverage or failure diagnostic.
    pub diagnostic: Option<String>,
    /// If superseded, records the newer job ID that replaced this job (`CASE-04`).
    pub superseded_by: Option<OpaqueId>,
}

impl DurableJob {
    fn to_json_value(&self) -> JsonValue {
        let mut m = BTreeMap::new();
        m.insert(
            "job_id".to_owned(),
            JsonValue::Str(self.job_id.as_str().to_owned()),
        );
        m.insert(
            "case_id".to_owned(),
            match &self.case_id {
                Some(c) => JsonValue::Str(c.as_str().to_owned()),
                None => JsonValue::Null,
            },
        );
        m.insert("kind".to_owned(), JsonValue::Str(self.kind.clone()));
        m.insert(
            "input_versions".to_owned(),
            JsonValue::Array(
                self.input_versions
                    .iter()
                    .map(|v| JsonValue::Str(v.as_str().to_owned()))
                    .collect(),
            ),
        );
        m.insert(
            "state".to_owned(),
            JsonValue::Str(self.state.as_str().to_owned()),
        );
        m.insert(
            "active_lease".to_owned(),
            match &self.active_lease {
                Some(l) => {
                    let mut lm = BTreeMap::new();
                    lm.insert(
                        "lease_epoch".to_owned(),
                        JsonValue::Int(l.lease_epoch as i64),
                    );
                    lm.insert("worker_id".to_owned(), JsonValue::Str(l.worker_id.clone()));
                    lm.insert("expires_at_ms".to_owned(), JsonValue::Int(l.expires_at_ms));
                    JsonValue::Object(lm)
                }
                None => JsonValue::Null,
            },
        );
        m.insert(
            "latest_epoch".to_owned(),
            JsonValue::Int(self.latest_epoch as i64),
        );
        m.insert(
            "attempt_count".to_owned(),
            JsonValue::Int(i64::from(self.attempt_count)),
        );
        m.insert(
            "max_attempts".to_owned(),
            JsonValue::Int(i64::from(self.max_attempts)),
        );
        m.insert(
            "progress_percent".to_owned(),
            JsonValue::Int(i64::from(self.progress_percent)),
        );
        m.insert(
            "output_versions".to_owned(),
            JsonValue::Array(
                self.output_versions
                    .iter()
                    .map(|v| JsonValue::Str(v.as_str().to_owned()))
                    .collect(),
            ),
        );
        m.insert(
            "diagnostic".to_owned(),
            match &self.diagnostic {
                Some(s) => JsonValue::Str(s.clone()),
                None => JsonValue::Null,
            },
        );
        m.insert(
            "superseded_by".to_owned(),
            match &self.superseded_by {
                Some(id) => JsonValue::Str(id.as_str().to_owned()),
                None => JsonValue::Null,
            },
        );
        JsonValue::Object(m)
    }

    fn from_json_value(val: &JsonValue) -> Result<Self, StoreError> {
        let m = val
            .as_object()
            .map_err(|e| StoreError::Domain(DomainError::Json(e)))?;
        let parse_vers = |key: &str| -> Result<Vec<VersionId>, StoreError> {
            let arr = JsonValue::req_array(m, key)
                .map_err(|e| StoreError::Domain(DomainError::Json(e)))?;
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                match item {
                    JsonValue::Str(s) => out.push(
                        VersionId::new(s.clone())
                            .map_err(|e| StoreError::Domain(DomainError::Id(e)))?,
                    ),
                    _ => {
                        return Err(StoreError::Domain(DomainError::Json(JsonError::Schema(
                            format!("'{key}' must contain strings"),
                        ))));
                    }
                }
            }
            Ok(out)
        };
        let active_lease = match JsonValue::opt_obj(m, "active_lease")
            .map_err(|e| StoreError::Domain(DomainError::Json(e)))?
        {
            Some(lm) => Some(JobLease {
                lease_epoch: JsonValue::req_i64(lm, "lease_epoch")
                    .map_err(|e| StoreError::Domain(DomainError::Json(e)))?
                    as u64,
                worker_id: JsonValue::req_str(lm, "worker_id")
                    .map_err(|e| StoreError::Domain(DomainError::Json(e)))?
                    .to_owned(),
                expires_at_ms: JsonValue::req_i64(lm, "expires_at_ms")
                    .map_err(|e| StoreError::Domain(DomainError::Json(e)))?,
            }),
            None => None,
        };
        Ok(Self {
            job_id: OpaqueId::new(
                JsonValue::req_str(m, "job_id")
                    .map_err(|e| StoreError::Domain(DomainError::Json(e)))?,
            )
            .map_err(|e| StoreError::Domain(DomainError::Id(e)))?,
            case_id: match JsonValue::opt_str(m, "case_id")
                .map_err(|e| StoreError::Domain(DomainError::Json(e)))?
            {
                Some(s) => {
                    Some(OpaqueId::new(s).map_err(|e| StoreError::Domain(DomainError::Id(e)))?)
                }
                None => None,
            },
            kind: JsonValue::req_str(m, "kind")
                .map_err(|e| StoreError::Domain(DomainError::Json(e)))?
                .to_owned(),
            input_versions: parse_vers("input_versions")?,
            state: JobState::from_tag(
                JsonValue::req_str(m, "state")
                    .map_err(|e| StoreError::Domain(DomainError::Json(e)))?,
            )?,
            active_lease,
            latest_epoch: JsonValue::req_i64(m, "latest_epoch")
                .map_err(|e| StoreError::Domain(DomainError::Json(e)))?
                as u64,
            attempt_count: JsonValue::req_i64(m, "attempt_count")
                .map_err(|e| StoreError::Domain(DomainError::Json(e)))?
                as u32,
            max_attempts: JsonValue::req_i64(m, "max_attempts")
                .map_err(|e| StoreError::Domain(DomainError::Json(e)))?
                as u32,
            progress_percent: JsonValue::req_i64(m, "progress_percent")
                .map_err(|e| StoreError::Domain(DomainError::Json(e)))?
                as u8,
            output_versions: parse_vers("output_versions")?,
            diagnostic: JsonValue::opt_str(m, "diagnostic")
                .map_err(|e| StoreError::Domain(DomainError::Json(e)))?,
            superseded_by: match JsonValue::opt_str(m, "superseded_by")
                .map_err(|e| StoreError::Domain(DomainError::Json(e)))?
            {
                Some(s) => {
                    Some(OpaqueId::new(s).map_err(|e| StoreError::Domain(DomainError::Id(e)))?)
                }
                None => None,
            },
        })
    }
}

/// Record of an applied schema migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaMigrationRecord {
    /// Source schema version.
    pub from_version: u32,
    /// Target schema version.
    pub to_version: u32,
    /// Applied timestamp (UTC ms).
    pub applied_at_ms: i64,
    /// Description of migration step.
    pub description: String,
}

impl SchemaMigrationRecord {
    fn to_json_value(&self) -> JsonValue {
        let mut m = BTreeMap::new();
        m.insert(
            "from_version".to_owned(),
            JsonValue::Int(i64::from(self.from_version)),
        );
        m.insert(
            "to_version".to_owned(),
            JsonValue::Int(i64::from(self.to_version)),
        );
        m.insert(
            "applied_at_ms".to_owned(),
            JsonValue::Int(self.applied_at_ms),
        );
        m.insert(
            "description".to_owned(),
            JsonValue::Str(self.description.clone()),
        );
        JsonValue::Object(m)
    }

    fn from_json_value(val: &JsonValue) -> Result<Self, StoreError> {
        let m = val
            .as_object()
            .map_err(|e| StoreError::Domain(DomainError::Json(e)))?;
        Ok(Self {
            from_version: JsonValue::req_i64(m, "from_version")
                .map_err(|e| StoreError::Domain(DomainError::Json(e)))?
                as u32,
            to_version: JsonValue::req_i64(m, "to_version")
                .map_err(|e| StoreError::Domain(DomainError::Json(e)))?
                as u32,
            applied_at_ms: JsonValue::req_i64(m, "applied_at_ms")
                .map_err(|e| StoreError::Domain(DomainError::Json(e)))?,
            description: JsonValue::req_str(m, "description")
                .map_err(|e| StoreError::Domain(DomainError::Json(e)))?
                .to_owned(),
        })
    }
}

/// Storage budget and resource limits for [`EvidenceStore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreBudget {
    /// Maximum total bytes across all stored blobs.
    pub max_total_blob_bytes: u64,
    /// Maximum bytes for a single blob.
    pub max_single_blob_bytes: u64,
}

impl Default for StoreBudget {
    fn default() -> Self {
        Self {
            max_total_blob_bytes: 256 * 1024 * 1024,
            max_single_blob_bytes: 32 * 1024 * 1024,
        }
    }
}

/// Portable backup snapshot with SHA-256 integrity index (`docs/architecture.md` Section 5.2).
///
/// Credentials are never included in backups.
#[derive(Debug, Clone)]
pub struct StoreBackupSnapshot {
    /// Schema version at backup time.
    pub schema_version: u32,
    /// Backup creation timestamp (UTC ms).
    pub created_at_ms: i64,
    /// Cloned domain objects.
    pub objects: Vec<DomainObject>,
    /// Cloned graph edges.
    pub edges: Vec<RelationEdge>,
    /// Cloned tombstones.
    pub tombstones: Vec<TombstoneRecord>,
    /// Cloned durable jobs.
    pub jobs: Vec<DurableJob>,
    /// Content-addressed blob map (`ContentHash -> raw bytes`) for available non-tombstoned blobs.
    pub blobs: BTreeMap<ContentHash, Vec<u8>>,
    /// Integrity index mapping each `ContentHash` to its expected byte length.
    pub integrity_index: BTreeMap<ContentHash, u64>,
}

/// Write durable file bytes using `File::create`, `write_all`, and `sync_all` (`A2`).
fn write_and_sync_file(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| StoreError::Io(e.to_string()))?;
    }
    let mut file = File::create(path).map_err(|e| StoreError::Io(e.to_string()))?;
    file.write_all(bytes)
        .map_err(|e| StoreError::Io(e.to_string()))?;
    file.sync_all().map_err(|e| StoreError::Io(e.to_string()))?;
    Ok(())
}

/// Transactional local evidence store with journaled disk persistence (`store.json`, `store.wal`,
/// `schema_migrations.json`), filesystem-backed content-addressed blobs (`blobs/`), and crash-safe
/// staging (`staging/`) (`#11`, `A1`–`A3`).
#[derive(Debug)]
pub struct EvidenceStore {
    root_dir: PathBuf,
    schema_version: u32,
    migrations: Vec<SchemaMigrationRecord>,
    budget: StoreBudget,
    graph: oia_domain::EvidenceGraph,
    blob_retention: BTreeMap<ContentHash, Vec<RetentionClass>>,
    blob_sizes: BTreeMap<ContentHash, u64>,
    tombstones: BTreeMap<VersionId, TombstoneRecord>,
    jobs: BTreeMap<OpaqueId, DurableJob>,
    pre_migration_backup: Option<StoreBackupSnapshot>,
}

impl EvidenceStore {
    /// Opens or initializes an [`EvidenceStore`] at `root_dir` (`A1`).
    ///
    /// - Rejects unknown future schemas (`schema_version == 0` or `> CURRENT_STORE_SCHEMA_VERSION`)
    ///   with [`StoreError::UnsupportedFutureSchema`].
    /// - If `store.wal` or `store.json` already exists in `root_dir`, loads and validates the
    ///   persisted schema version, domain objects, graph edges, tombstones, jobs, blob retention
    ///   metadata, and `schema_migrations.json` from disk.
    /// - Cleans up any leftover uncommitted files in `staging/` from an interrupted prior run.
    pub fn open(
        root_dir: impl AsRef<Path>,
        schema_version: u32,
        budget: StoreBudget,
    ) -> Result<Self, StoreError> {
        if schema_version == 0 || schema_version > CURRENT_STORE_SCHEMA_VERSION {
            return Err(StoreError::UnsupportedFutureSchema {
                found: schema_version,
                max_supported: CURRENT_STORE_SCHEMA_VERSION,
            });
        }
        let root = root_dir.as_ref().to_path_buf();
        fs::create_dir_all(root.join("blobs")).map_err(|e| StoreError::Io(e.to_string()))?;
        fs::create_dir_all(root.join("staging")).map_err(|e| StoreError::Io(e.to_string()))?;

        let mut store = Self {
            root_dir: root.clone(),
            schema_version,
            migrations: Vec::new(),
            budget,
            graph: oia_domain::EvidenceGraph::new(),
            blob_retention: BTreeMap::new(),
            blob_sizes: BTreeMap::new(),
            tombstones: BTreeMap::new(),
            jobs: BTreeMap::new(),
            pre_migration_backup: None,
        };
        store.cleanup_staging()?;

        let wal_path = root.join("store.wal");
        let store_path = root.join("store.json");
        if wal_path.exists() {
            // Replay uncommitted write-ahead journal into store.json and remove wal.
            let wal_bytes = fs::read(&wal_path).map_err(|e| StoreError::Io(e.to_string()))?;
            write_and_sync_file(&store_path, &wal_bytes)?;
            let _ = fs::remove_file(&wal_path);
        }

        if store_path.exists() {
            store.load_state_from_disk()?;
        } else {
            store.persist_state()?;
        }
        Ok(store)
    }

    fn persist_state(&self) -> Result<(), StoreError> {
        let mut root_map = BTreeMap::new();
        root_map.insert(
            "schema_version".to_owned(),
            JsonValue::Int(i64::from(self.schema_version)),
        );

        let mut objs_arr = Vec::new();
        for (_, obj) in self.graph.all_versions() {
            objs_arr.push(obj.to_json_value().map_err(StoreError::Domain)?);
        }
        root_map.insert("objects".to_owned(), JsonValue::Array(objs_arr));

        let edges_arr: Vec<JsonValue> = self
            .graph
            .edges()
            .iter()
            .map(RelationEdge::to_json_value)
            .collect();
        root_map.insert("edges".to_owned(), JsonValue::Array(edges_arr));

        let ts_arr: Vec<JsonValue> = self
            .tombstones
            .values()
            .map(TombstoneRecord::to_json_value)
            .collect();
        root_map.insert("tombstones".to_owned(), JsonValue::Array(ts_arr));

        let jobs_arr: Vec<JsonValue> = self.jobs.values().map(DurableJob::to_json_value).collect();
        root_map.insert("jobs".to_owned(), JsonValue::Array(jobs_arr));

        let mut sizes_map = BTreeMap::new();
        for (h, &sz) in &self.blob_sizes {
            sizes_map.insert(h.to_hex(), JsonValue::Int(sz as i64));
        }
        root_map.insert("blob_sizes".to_owned(), JsonValue::Object(sizes_map));

        let mut ret_map = BTreeMap::new();
        for (h, classes) in &self.blob_retention {
            ret_map.insert(
                h.to_hex(),
                JsonValue::Array(
                    classes
                        .iter()
                        .copied()
                        .map(RetentionClass::to_json_value)
                        .collect(),
                ),
            );
        }
        root_map.insert("blob_retention".to_owned(), JsonValue::Object(ret_map));

        let state_json = JsonValue::Object(root_map)
            .to_canonical_json()
            .map_err(|e| StoreError::Domain(DomainError::Json(e)))?;

        // 1. Write and fsync WAL journal first (`store.wal`).
        let wal_path = self.root_dir.join("store.wal");
        write_and_sync_file(&wal_path, state_json.as_bytes())?;

        // 2. Write and fsync temporary snapshot (`store.json.tmp`), then rename to `store.json`.
        let tmp_path = self.root_dir.join("store.json.tmp");
        let final_path = self.root_dir.join("store.json");
        write_and_sync_file(&tmp_path, state_json.as_bytes())?;
        fs::rename(&tmp_path, &final_path).map_err(|e| StoreError::Io(e.to_string()))?;

        // 3. Persist `schema_migrations.json`.
        let mig_arr: Vec<JsonValue> = self
            .migrations
            .iter()
            .map(SchemaMigrationRecord::to_json_value)
            .collect();
        let mig_json = JsonValue::Array(mig_arr)
            .to_canonical_json()
            .map_err(|e| StoreError::Domain(DomainError::Json(e)))?;
        write_and_sync_file(
            &self.root_dir.join("schema_migrations.json"),
            mig_json.as_bytes(),
        )?;

        // 4. Remove committed WAL journal.
        let _ = fs::remove_file(&wal_path);
        Ok(())
    }

    fn load_state_from_disk(&mut self) -> Result<(), StoreError> {
        let store_path = self.root_dir.join("store.json");
        let raw = fs::read_to_string(&store_path).map_err(|e| StoreError::Io(e.to_string()))?;
        let budget = JsonBudget {
            max_input_bytes: 64 * 1024 * 1024,
            max_depth: 64,
            max_container_items: 100_000,
            max_string_bytes: 4 * 1024 * 1024,
        };
        let val = JsonValue::parse_bounded(&raw, budget)
            .map_err(|e| StoreError::Domain(DomainError::Json(e)))?;
        let m = val
            .as_object()
            .map_err(|e| StoreError::Domain(DomainError::Json(e)))?;

        let disk_sv = JsonValue::req_i64(m, "schema_version")
            .map_err(|e| StoreError::Domain(DomainError::Json(e)))?;
        let disk_sv_u32 = u32::try_from(disk_sv).unwrap_or(u32::MAX);
        if disk_sv_u32 == 0 || disk_sv_u32 > CURRENT_STORE_SCHEMA_VERSION {
            return Err(StoreError::UnsupportedFutureSchema {
                found: disk_sv_u32,
                max_supported: CURRENT_STORE_SCHEMA_VERSION,
            });
        }
        self.schema_version = disk_sv_u32;

        let mut graph = oia_domain::EvidenceGraph::new();
        for item in JsonValue::req_array(m, "objects")
            .map_err(|e| StoreError::Domain(DomainError::Json(e)))?
        {
            let obj = DomainObject::from_json_value(item).map_err(StoreError::Domain)?;
            graph
                .restore_object_unchecked(obj)
                .map_err(StoreError::Domain)?;
        }
        for item in JsonValue::req_array(m, "edges")
            .map_err(|e| StoreError::Domain(DomainError::Json(e)))?
        {
            let edge = RelationEdge::from_json_value(item).map_err(StoreError::Domain)?;
            graph.restore_edge_unchecked(edge);
        }
        self.graph = graph;

        self.tombstones.clear();
        for item in JsonValue::req_array(m, "tombstones")
            .map_err(|e| StoreError::Domain(DomainError::Json(e)))?
        {
            let ts = TombstoneRecord::from_json_value(item)?;
            self.tombstones.insert(ts.artifact_version_id.clone(), ts);
        }

        self.jobs.clear();
        for item in
            JsonValue::req_array(m, "jobs").map_err(|e| StoreError::Domain(DomainError::Json(e)))?
        {
            let job = DurableJob::from_json_value(item)?;
            self.jobs.insert(job.job_id.clone(), job);
        }

        self.blob_sizes.clear();
        let sizes_obj = JsonValue::req_obj(m, "blob_sizes")
            .map_err(|e| StoreError::Domain(DomainError::Json(e)))?;
        for (hex, v) in sizes_obj {
            let hash =
                ContentHash::from_hex(hex).map_err(|e| StoreError::Domain(DomainError::Hash(e)))?;
            let sz = match v {
                JsonValue::Int(n) if *n >= 0 => *n as u64,
                _ => {
                    return Err(StoreError::Domain(DomainError::Json(JsonError::Schema(
                        "invalid blob_sizes entry".to_owned(),
                    ))));
                }
            };
            self.blob_sizes.insert(hash, sz);
        }

        self.blob_retention.clear();
        let ret_obj = JsonValue::req_obj(m, "blob_retention")
            .map_err(|e| StoreError::Domain(DomainError::Json(e)))?;
        for (hex, v) in ret_obj {
            let hash =
                ContentHash::from_hex(hex).map_err(|e| StoreError::Domain(DomainError::Hash(e)))?;
            let arr = match v {
                JsonValue::Array(a) => a,
                _ => {
                    return Err(StoreError::Domain(DomainError::Json(JsonError::Schema(
                        "blob_retention value must be array".to_owned(),
                    ))));
                }
            };
            let mut classes = Vec::with_capacity(arr.len());
            for item in arr {
                classes.push(RetentionClass::from_json_value(item)?);
            }
            self.blob_retention.insert(hash, classes);
        }

        self.migrations.clear();
        let mig_path = self.root_dir.join("schema_migrations.json");
        if mig_path.exists() {
            let mig_raw =
                fs::read_to_string(&mig_path).map_err(|e| StoreError::Io(e.to_string()))?;
            let mig_val = JsonValue::parse_bounded(&mig_raw, budget)
                .map_err(|e| StoreError::Domain(DomainError::Json(e)))?;
            if let JsonValue::Array(arr) = mig_val {
                for item in &arr {
                    self.migrations
                        .push(SchemaMigrationRecord::from_json_value(item)?);
                }
            }
        }
        Ok(())
    }

    /// Returns the current store schema version.
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Returns recorded schema migrations.
    #[must_use]
    pub fn migrations(&self) -> &[SchemaMigrationRecord] {
        &self.migrations
    }

    /// Returns a reference to the pre-migration backup snapshot if a migration has been applied.
    #[must_use]
    pub const fn pre_migration_backup(&self) -> Option<&StoreBackupSnapshot> {
        self.pre_migration_backup.as_ref()
    }

    /// Applies a forward schema migration to `target_version`, taking a pre-migration backup first (`docs/architecture.md` Section 13).
    pub fn migrate_schema(
        &mut self,
        target_version: u32,
        now_ms: i64,
        description: impl Into<String>,
    ) -> Result<(), StoreError> {
        if target_version > CURRENT_STORE_SCHEMA_VERSION {
            return Err(StoreError::UnsupportedFutureSchema {
                found: target_version,
                max_supported: CURRENT_STORE_SCHEMA_VERSION,
            });
        }
        if target_version <= self.schema_version {
            return Err(StoreError::InvalidMigrationTarget {
                from: self.schema_version,
                to: target_version,
            });
        }
        let backup = self.create_backup(now_ms)?;
        self.pre_migration_backup = Some(backup);
        let from = self.schema_version;
        self.schema_version = target_version;
        self.migrations.push(SchemaMigrationRecord {
            from_version: from,
            to_version: target_version,
            applied_at_ms: now_ms,
            description: description.into(),
        });
        self.persist_state()
    }

    /// Returns a shared reference to the underlying [`oia_domain::EvidenceGraph`].
    #[must_use]
    pub const fn graph(&self) -> &oia_domain::EvidenceGraph {
        &self.graph
    }

    /// Returns total bytes currently stored across all active blobs.
    #[must_use]
    pub fn total_blob_bytes(&self) -> u64 {
        self.blob_sizes.values().sum()
    }

    /// Cleans up any uncommitted files in the `staging/` directory after a rollback or crash.
    pub fn cleanup_staging(&self) -> Result<usize, StoreError> {
        let staging_dir = self.root_dir.join("staging");
        if !staging_dir.exists() {
            return Ok(0);
        }
        let mut removed = 0usize;
        let entries = fs::read_dir(&staging_dir).map_err(|e| StoreError::Io(e.to_string()))?;
        for entry_res in entries {
            let entry = entry_res.map_err(|e| StoreError::Io(e.to_string()))?;
            let path = entry.path();
            if path.is_file() {
                fs::remove_file(&path).map_err(|e| StoreError::Io(e.to_string()))?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    fn blob_path(&self, hash: &ContentHash) -> PathBuf {
        let hex = hash.to_hex();
        let prefix = &hex[..2];
        self.root_dir.join("blobs").join(prefix).join(&hex)
    }

    fn stage_blob_bytes(&self, bytes: &[u8]) -> Result<(ContentHash, PathBuf), StoreError> {
        let len = bytes.len() as u64;
        if len > self.budget.max_single_blob_bytes {
            return Err(StoreError::BlobTooLarge {
                actual: len,
                max: self.budget.max_single_blob_bytes,
            });
        }
        let hash = ContentHash::compute(bytes);
        if !self.blob_sizes.contains_key(&hash)
            && self.total_blob_bytes().saturating_add(len) > self.budget.max_total_blob_bytes
        {
            return Err(StoreError::StorageExhausted {
                requested: len,
                available: self
                    .budget
                    .max_total_blob_bytes
                    .saturating_sub(self.total_blob_bytes()),
            });
        }
        let stage_path = self
            .root_dir
            .join("staging")
            .join(format!("{}.part", hash.to_hex()));
        write_and_sync_file(&stage_path, bytes)?;
        Ok((hash, stage_path))
    }

    fn promote_staged_blob(
        &mut self,
        hash: &ContentHash,
        stage_path: &Path,
        byte_len: u64,
        retention: RetentionClass,
    ) -> Result<(), StoreError> {
        let final_path = self.blob_path(hash);
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).map_err(|e| StoreError::Io(e.to_string()))?;
        }
        if final_path.exists() {
            let existing = fs::read(&final_path).map_err(|e| StoreError::Io(e.to_string()))?;
            if existing.len() as u64 != byte_len || !hash.verify_bytes(&existing) {
                let _ = fs::remove_file(stage_path);
                return Err(StoreError::CorruptBlobOnDisk(hash.clone()));
            }
            if stage_path.exists() {
                let _ = fs::remove_file(stage_path);
            }
        } else {
            fs::rename(stage_path, &final_path).map_err(|e| StoreError::Io(e.to_string()))?;
        }
        self.blob_sizes.insert(hash.clone(), byte_len);
        self.blob_retention
            .entry(hash.clone())
            .or_default()
            .push(retention);
        Ok(())
    }

    /// Stages, verifies, and promotes a content-addressed blob directly into the store (`C10`).
    pub fn put_verified_blob(
        &mut self,
        expected_hash: &ContentHash,
        raw_bytes: &[u8],
        retention: RetentionClass,
    ) -> Result<ContentHash, StoreError> {
        let (computed_hash, stage_path) = self.stage_blob_bytes(raw_bytes)?;
        if &computed_hash != expected_hash {
            let _ = fs::remove_file(&stage_path);
            return Err(StoreError::HashMismatch {
                expected: expected_hash.clone(),
                actual: computed_hash,
            });
        }
        self.promote_staged_blob(
            &computed_hash,
            &stage_path,
            raw_bytes.len() as u64,
            retention,
        )?;
        self.persist_state()?;
        Ok(computed_hash)
    }

    /// Atomically commits a [`SourceRecord`], its original [`Artifact`] metadata, and its raw bytes
    /// into content-addressed storage (`#11`, `A2`).
    ///
    /// Validates both objects and checks graph insertion preconditions on a cloned graph before
    /// promoting the staged blob or mutating `self.graph`, ensuring zero partial commits on failure.
    pub fn commit_source_and_artifact(
        &mut self,
        source_record: SourceRecord,
        artifact: Artifact,
        raw_bytes: &[u8],
        retention: RetentionClass,
    ) -> Result<ContentHash, StoreError> {
        let (computed_hash, stage_path) = self.stage_blob_bytes(raw_bytes)?;

        let validate_step =
            |graph: &oia_domain::EvidenceGraph| -> Result<oia_domain::EvidenceGraph, StoreError> {
                if computed_hash != artifact.content_hash {
                    return Err(StoreError::HashMismatch {
                        expected: artifact.content_hash.clone(),
                        actual: computed_hash.clone(),
                    });
                }
                if raw_bytes.len() as u64 != artifact.byte_len {
                    return Err(StoreError::ByteLengthMismatch {
                        expected: artifact.byte_len,
                        actual: raw_bytes.len() as u64,
                    });
                }
                if let oia_domain::RetainedContent::RetainedBlob {
                    content_hash: ref sr_hash,
                    byte_len: sr_len,
                    ..
                } = source_record.content
                {
                    if sr_hash != &computed_hash {
                        return Err(StoreError::HashMismatch {
                            expected: sr_hash.clone(),
                            actual: computed_hash.clone(),
                        });
                    }
                    if sr_len != artifact.byte_len {
                        return Err(StoreError::ByteLengthMismatch {
                            expected: sr_len,
                            actual: artifact.byte_len,
                        });
                    }
                }
                // Dry-run both insertions on a cloned graph before promoting blob bytes (`A2`).
                let mut next_graph = graph.clone();
                next_graph
                    .insert_object(DomainObject::SourceRecord(source_record.clone()), None)
                    .map_err(StoreError::Domain)?;
                next_graph
                    .insert_object(DomainObject::Artifact(artifact.clone()), None)
                    .map_err(StoreError::Domain)?;
                Ok(next_graph)
            };

        let next_graph = match validate_step(&self.graph) {
            Ok(g) => g,
            Err(err) => {
                let _ = fs::remove_file(&stage_path);
                return Err(err);
            }
        };

        self.promote_staged_blob(
            &computed_hash,
            &stage_path,
            raw_bytes.len() as u64,
            retention,
        )?;
        self.graph = next_graph;
        self.persist_state()?;
        Ok(computed_hash)
    }

    /// Commits a validated [`DomainObject`] into the store's evidence graph and persists to disk.
    pub fn put_object(
        &mut self,
        object: DomainObject,
        supersession_reason: Option<&str>,
    ) -> Result<VersionId, StoreError> {
        let ver = self
            .graph
            .insert_object(object, supersession_reason)
            .map_err(StoreError::Domain)?;
        self.persist_state()?;
        Ok(ver)
    }

    /// Adds a validated [`RelationEdge`] to the store's evidence graph and persists to disk.
    pub fn add_edge(&mut self, edge: RelationEdge) -> Result<(), StoreError> {
        self.graph.add_edge(edge).map_err(StoreError::Domain)?;
        self.persist_state()
    }

    /// Reads and verifies the SHA-256 integrity of a stored blob (`#11`, `A17`).
    ///
    /// Never substitutes placeholder bytes if the blob was tombstoned or is missing.
    pub fn read_verified_blob(&self, hash: &ContentHash) -> Result<Vec<u8>, StoreError> {
        if self.tombstones.values().any(|t| &t.content_hash == hash)
            && !self.blob_sizes.contains_key(hash)
        {
            return Err(StoreError::BlobTombstoned(hash.clone()));
        }
        let path = self.blob_path(hash);
        if !path.exists() {
            return Err(StoreError::MissingBlob(hash.clone()));
        }
        let bytes = fs::read(&path).map_err(|e| StoreError::Io(e.to_string()))?;
        if !hash.verify_bytes(&bytes) {
            return Err(StoreError::CorruptBlobOnDisk(hash.clone()));
        }
        Ok(bytes)
    }

    /// Deletes an artifact's underlying bytes while recording an immutable [`TombstoneRecord`],
    /// marking the artifact [`AvailabilityState::Removed`], and flagging dependent records
    /// `NeedsReassessment` without pretending the original bytes are still present (`#11`, `A3`).
    pub fn tombstone_artifact(
        &mut self,
        artifact_version_id: &VersionId,
        deleted_by: ActorId,
        deleted_at_ms: i64,
        reason: impl Into<String>,
    ) -> Result<TombstoneRecord, StoreError> {
        let obj = self
            .graph
            .get_version(artifact_version_id)
            .ok_or_else(|| StoreError::VersionNotFound(artifact_version_id.clone()))?;
        let DomainObject::Artifact(art) = obj else {
            return Err(StoreError::NotAnArtifactVersion(
                artifact_version_id.clone(),
            ));
        };
        let hash = art.content_hash.clone();
        let reason_str = reason.into();

        // Mark this artifact version as `Removed` BEFORE checking whether any other live artifact
        // still pins the same content hash (`A3`).
        self.graph
            .set_version_availability(artifact_version_id, AvailabilityState::Removed)
            .map_err(StoreError::Domain)?;

        let mut other_live_refs = false;
        for (ver, candidate) in self.graph.all_versions() {
            if ver != artifact_version_id
                && !self.tombstones.contains_key(ver)
                && let DomainObject::Artifact(other_art) = candidate
                && other_art.content_hash == hash
                && other_art.envelope.availability == AvailabilityState::Available
            {
                other_live_refs = true;
                break;
            }
        }

        if !other_live_refs {
            let path = self.blob_path(&hash);
            if path.exists() {
                fs::remove_file(&path).map_err(|e| StoreError::Io(e.to_string()))?;
            }
            self.blob_sizes.remove(&hash);
            self.blob_retention.remove(&hash);
            // Also purge the deleted bytes from any in-memory pre-migration backup (`A3`, `AGENTS.md` §5).
            if let Some(ref mut backup) = self.pre_migration_backup {
                backup.blobs.remove(&hash);
                backup.integrity_index.remove(&hash);
            }
        }

        self.graph.propagate_reassessment(
            artifact_version_id,
            &format!("artifact tombstoned: {reason_str}"),
            deleted_at_ms,
        );

        let tombstone = TombstoneRecord {
            artifact_version_id: artifact_version_id.clone(),
            content_hash: hash,
            deleted_at_ms,
            deleted_by,
            reason: reason_str,
        };
        self.tombstones
            .insert(artifact_version_id.clone(), tombstone.clone());
        self.persist_state()?;
        Ok(tombstone)
    }

    /// Garbage-collects expired `RoutineBuffer` and `TemporaryExtractionCache` blobs that have
    /// no protected (`PinnedCaseEvidence` or `IssuedReportSnapshot`) retention references (`#11`).
    pub fn collect_expired_blobs(&mut self, now_ms: i64) -> Result<Vec<ContentHash>, StoreError> {
        let mut evicted = Vec::new();
        let hashes: Vec<ContentHash> = self.blob_retention.keys().cloned().collect();
        for hash in hashes {
            let Some(classes) = self.blob_retention.get(&hash) else {
                continue;
            };
            let has_protected = classes.iter().copied().any(RetentionClass::is_protected);
            let all_expired =
                !classes.is_empty() && classes.iter().copied().all(|c| c.is_expired_at(now_ms));
            if !has_protected && all_expired {
                let path = self.blob_path(&hash);
                if path.exists() {
                    fs::remove_file(&path).map_err(|e| StoreError::Io(e.to_string()))?;
                }
                self.blob_retention.remove(&hash);
                self.blob_sizes.remove(&hash);
                evicted.push(hash);
            }
        }
        if !evicted.is_empty() {
            self.persist_state()?;
        }
        Ok(evicted)
    }

    /// Creates a verified backup snapshot of the store, excluding any raw credentials (`#11`).
    pub fn create_backup(&self, now_ms: i64) -> Result<StoreBackupSnapshot, StoreError> {
        let mut blobs = BTreeMap::new();
        let mut integrity_index = BTreeMap::new();
        for (hash, &expected_len) in &self.blob_sizes {
            let bytes = self.read_verified_blob(hash)?;
            if bytes.len() as u64 != expected_len {
                return Err(StoreError::ByteLengthMismatch {
                    expected: expected_len,
                    actual: bytes.len() as u64,
                });
            }
            integrity_index.insert(hash.clone(), expected_len);
            blobs.insert(hash.clone(), bytes);
        }

        let objects = self
            .graph
            .all_versions()
            .map(|(_, obj)| obj.clone())
            .collect();
        let edges = self.graph.edges().to_vec();
        let tombstones = self.tombstones.values().cloned().collect();
        let jobs = self.jobs.values().cloned().collect();

        Ok(StoreBackupSnapshot {
            schema_version: self.schema_version,
            created_at_ms: now_ms,
            objects,
            edges,
            tombstones,
            jobs,
            blobs,
            integrity_index,
        })
    }

    /// Restores a [`StoreBackupSnapshot`] into `target_dir` after validating schema compatibility
    /// and every blob's SHA-256 digest (`#11`).
    ///
    /// If any integrity check fails, returns an error without creating a partially restored store.
    pub fn restore_from_backup(
        target_dir: impl AsRef<Path>,
        snapshot: &StoreBackupSnapshot,
        budget: StoreBudget,
    ) -> Result<Self, StoreError> {
        if snapshot.schema_version == 0 || snapshot.schema_version > CURRENT_STORE_SCHEMA_VERSION {
            return Err(StoreError::UnsupportedFutureSchema {
                found: snapshot.schema_version,
                max_supported: CURRENT_STORE_SCHEMA_VERSION,
            });
        }
        for (hash, &expected_len) in &snapshot.integrity_index {
            let bytes = snapshot
                .blobs
                .get(hash)
                .ok_or_else(|| StoreError::MissingBlob(hash.clone()))?;
            if bytes.len() as u64 != expected_len || !hash.verify_bytes(bytes) {
                return Err(StoreError::CorruptBlobOnDisk(hash.clone()));
            }
        }

        let mut store = Self::open(target_dir, snapshot.schema_version, budget)?;
        for (hash, bytes) in &snapshot.blobs {
            let (_, stage_path) = store.stage_blob_bytes(bytes)?;
            store.promote_staged_blob(
                hash,
                &stage_path,
                bytes.len() as u64,
                RetentionClass::PinnedCaseEvidence,
            )?;
        }
        for obj in &snapshot.objects {
            store
                .graph
                .restore_object_unchecked(obj.clone())
                .map_err(StoreError::Domain)?;
        }
        for edge in &snapshot.edges {
            store.graph.restore_edge_unchecked(edge.clone());
        }
        for ts in &snapshot.tombstones {
            store
                .tombstones
                .insert(ts.artifact_version_id.clone(), ts.clone());
        }
        for job in &snapshot.jobs {
            store.jobs.insert(job.job_id.clone(), job.clone());
        }
        store.persist_state()?;
        Ok(store)
    }

    // -------------------------------------------------------------------------
    // Durable Jobs & Lease Management (#14, A01)
    // -------------------------------------------------------------------------

    /// Enqueues a new [`DurableJob`].
    pub fn enqueue_job(
        &mut self,
        job_id: OpaqueId,
        case_id: Option<OpaqueId>,
        kind: impl Into<String>,
        input_versions: Vec<VersionId>,
        max_attempts: u32,
    ) -> Result<&DurableJob, StoreError> {
        if self.jobs.contains_key(&job_id) {
            return Err(StoreError::DuplicateJobId(job_id));
        }
        let job = DurableJob {
            job_id: job_id.clone(),
            case_id,
            kind: kind.into(),
            input_versions,
            state: JobState::Queued,
            active_lease: None,
            latest_epoch: 0,
            attempt_count: 0,
            max_attempts: max_attempts.max(1),
            progress_percent: 0,
            output_versions: Vec::new(),
            diagnostic: None,
            superseded_by: None,
        };
        self.jobs.insert(job_id.clone(), job);
        self.persist_state()?;
        self.jobs
            .get(&job_id)
            .ok_or(StoreError::JobNotFound(job_id))
    }

    /// Acquires an execution lease on a `Queued` job for `worker_id` until `expires_at_ms`.
    pub fn acquire_job_lease(
        &mut self,
        job_id: &OpaqueId,
        worker_id: impl Into<String>,
        expires_at_ms: i64,
    ) -> Result<JobLease, StoreError> {
        let lease = {
            let job = self
                .jobs
                .get_mut(job_id)
                .ok_or_else(|| StoreError::JobNotFound(job_id.clone()))?;
            if job.state != JobState::Queued {
                return Err(StoreError::InvalidJobTransition {
                    job_id: job_id.clone(),
                    current: job.state,
                });
            }
            job.latest_epoch += 1;
            job.attempt_count += 1;
            job.state = JobState::Running;
            let lease = JobLease {
                lease_epoch: job.latest_epoch,
                worker_id: worker_id.into(),
                expires_at_ms,
            };
            job.active_lease = Some(lease.clone());
            lease
        };
        self.persist_state()?;
        Ok(lease)
    }

    /// Cancels a running or queued job and revokes its active lease so a late worker cannot
    /// publish results (`CASE-04`, `A01`).
    pub fn cancel_job(
        &mut self,
        job_id: &OpaqueId,
        reason: impl Into<String>,
    ) -> Result<(), StoreError> {
        {
            let job = self
                .jobs
                .get_mut(job_id)
                .ok_or_else(|| StoreError::JobNotFound(job_id.clone()))?;
            if job.state.is_terminal() {
                return Ok(());
            }
            job.latest_epoch += 1;
            job.active_lease = None;
            job.state = JobState::Cancelled;
            job.diagnostic = Some(reason.into());
        }
        self.persist_state()
    }

    /// Supersedes `old_job_id` with `new_job_id`, revoking `old_job_id`'s lease so late results
    /// from the old operation can never overwrite the newer context (`CASE-04`, `A01`).
    pub fn supersede_job(
        &mut self,
        old_job_id: &OpaqueId,
        new_job_id: OpaqueId,
    ) -> Result<(), StoreError> {
        {
            let job = self
                .jobs
                .get_mut(old_job_id)
                .ok_or_else(|| StoreError::JobNotFound(old_job_id.clone()))?;
            job.latest_epoch += 1;
            job.active_lease = None;
            job.state = JobState::Superseded;
            job.superseded_by = Some(new_job_id);
        }
        self.persist_state()
    }

    /// Completes a running job only if `lease_epoch` matches the active unrevoked, non-expired lease (`#14`, `A01`).
    ///
    /// If the job was cancelled, superseded, or its lease expired, rejects publication with
    /// [`StoreError::StaleOrRevokedLease`] without committing `output_objects`.
    pub fn complete_job_with_outputs(
        &mut self,
        job_id: &OpaqueId,
        lease_epoch: u64,
        now_ms: i64,
        partial_limitation: Option<String>,
        output_objects: Vec<DomainObject>,
    ) -> Result<JobState, StoreError> {
        {
            let job = self
                .jobs
                .get(job_id)
                .ok_or_else(|| StoreError::JobNotFound(job_id.clone()))?;
            if job.state != JobState::Running {
                return Err(StoreError::StaleOrRevokedLease {
                    job_id: job_id.clone(),
                    attempted_epoch: lease_epoch,
                });
            }
            let Some(ref lease) = job.active_lease else {
                return Err(StoreError::StaleOrRevokedLease {
                    job_id: job_id.clone(),
                    attempted_epoch: lease_epoch,
                });
            };
            if lease.lease_epoch != lease_epoch || now_ms > lease.expires_at_ms {
                return Err(StoreError::StaleOrRevokedLease {
                    job_id: job_id.clone(),
                    attempted_epoch: lease_epoch,
                });
            }
        }

        let mut next_graph = self.graph.clone();
        let mut published_versions = Vec::with_capacity(output_objects.len());
        for obj in output_objects {
            let ver = next_graph
                .insert_object(obj, None)
                .map_err(StoreError::Domain)?;
            published_versions.push(ver);
        }
        self.graph = next_graph;

        let final_state = {
            let job = self
                .jobs
                .get_mut(job_id)
                .ok_or_else(|| StoreError::JobNotFound(job_id.clone()))?;
            job.active_lease = None;
            job.progress_percent = 100;
            job.output_versions = published_versions;
            if let Some(lim) = partial_limitation {
                job.state = JobState::PartiallySucceeded;
                job.diagnostic = Some(lim);
            } else {
                job.state = JobState::Succeeded;
            }
            job.state
        };
        self.persist_state()?;
        Ok(final_state)
    }

    /// Recovers interrupted or lease-expired `Running` jobs on restart or watchdog tick (`#14`).
    ///
    /// Never marks incomplete work as succeeded; re-queues jobs with remaining retry budget or
    /// marks them `Failed`.
    pub fn recover_interrupted_jobs(
        &mut self,
        now_ms: i64,
        force_restart_recovery: bool,
    ) -> Vec<OpaqueId> {
        let mut recovered = Vec::new();
        for (id, job) in &mut self.jobs {
            if job.state == JobState::Running {
                let expired = job
                    .active_lease
                    .as_ref()
                    .is_none_or(|l| force_restart_recovery || now_ms > l.expires_at_ms);
                if expired {
                    job.latest_epoch += 1;
                    job.active_lease = None;
                    if job.attempt_count < job.max_attempts {
                        job.state = JobState::Queued;
                        job.diagnostic =
                            Some("Recovered after interrupted/expired lease; re-queued".to_owned());
                    } else {
                        job.state = JobState::Failed;
                        job.diagnostic =
                            Some("Max attempts exhausted after lease interruption".to_owned());
                    }
                    recovered.push(id.clone());
                }
            }
        }
        if !recovered.is_empty() {
            let _ = self.persist_state();
        }
        recovered
    }

    /// Looks up a [`DurableJob`] by ID.
    #[must_use]
    pub fn get_job(&self, job_id: &OpaqueId) -> Option<&DurableJob> {
        self.jobs.get(job_id)
    }

    /// Returns all tombstones.
    #[must_use]
    pub const fn tombstones(&self) -> &BTreeMap<VersionId, TombstoneRecord> {
        &self.tombstones
    }

    /// Returns the set of content hashes currently stored on disk.
    #[must_use]
    pub fn stored_blob_hashes(&self) -> BTreeSet<ContentHash> {
        self.blob_sizes.keys().cloned().collect()
    }
}

/// Typed errors for `oia-store`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// Underlying domain error.
    Domain(DomainError),
    /// Filesystem I/O failure.
    Io(String),
    /// Store or backup used an unsupported future schema version.
    UnsupportedFutureSchema {
        /// Version found.
        found: u32,
        /// Maximum supported version.
        max_supported: u32,
    },
    /// Invalid schema migration target.
    InvalidMigrationTarget {
        /// Current schema version.
        from: u32,
        /// Requested target version.
        to: u32,
    },
    /// Single blob exceeded `StoreBudget::max_single_blob_bytes`.
    BlobTooLarge {
        /// Actual byte length.
        actual: u64,
        /// Maximum allowed byte length.
        max: u64,
    },
    /// Total store blob budget exhausted.
    StorageExhausted {
        /// Requested byte count.
        requested: u64,
        /// Available remaining byte budget.
        available: u64,
    },
    /// Computed SHA-256 did not match declared artifact content hash.
    HashMismatch {
        /// Declared content hash.
        expected: ContentHash,
        /// Computed content hash.
        actual: ContentHash,
    },
    /// Byte length did not match declared artifact length.
    ByteLengthMismatch {
        /// Declared byte length.
        expected: u64,
        /// Actual byte length.
        actual: u64,
    },
    /// Requested blob was explicitly tombstoned/deleted.
    BlobTombstoned(ContentHash),
    /// Expected blob file is missing from storage.
    MissingBlob(ContentHash),
    /// Stored blob failed SHA-256 verification on read/restore.
    CorruptBlobOnDisk(ContentHash),
    /// Referenced version ID was not found.
    VersionNotFound(VersionId),
    /// Target version ID is not a `DomainObject::Artifact`.
    NotAnArtifactVersion(VersionId),
    /// Duplicate job ID.
    DuplicateJobId(OpaqueId),
    /// Job ID not found.
    JobNotFound(OpaqueId),
    /// Invalid job state transition.
    InvalidJobTransition {
        /// Job ID.
        job_id: OpaqueId,
        /// Current state.
        current: JobState,
    },
    /// Worker attempted to publish results with a revoked, superseded, cancelled, or expired lease.
    StaleOrRevokedLease {
        /// Job ID.
        job_id: OpaqueId,
        /// Lease epoch presented by the stale worker.
        attempted_epoch: u64,
    },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(e) => write!(f, "store domain error: {e}"),
            Self::Io(e) => write!(f, "store I/O error: {e}"),
            Self::UnsupportedFutureSchema {
                found,
                max_supported,
            } => write!(
                f,
                "unsupported schema version {found} (max supported is {max_supported})"
            ),
            Self::InvalidMigrationTarget { from, to } => {
                write!(f, "cannot migrate schema from {from} to {to}")
            }
            Self::BlobTooLarge { actual, max } => {
                write!(
                    f,
                    "blob size {actual} exceeds single-blob limit {max} bytes"
                )
            }
            Self::StorageExhausted {
                requested,
                available,
            } => write!(
                f,
                "storage quota exhausted: requested {requested} bytes, only {available} available"
            ),
            Self::HashMismatch { expected, actual } => {
                write!(f, "SHA-256 mismatch: expected {expected}, got {actual}")
            }
            Self::ByteLengthMismatch { expected, actual } => {
                write!(f, "byte length mismatch: expected {expected}, got {actual}")
            }
            Self::BlobTombstoned(h) => write!(f, "blob {h} has been deleted (tombstoned)"),
            Self::MissingBlob(h) => write!(f, "blob {h} is missing from storage"),
            Self::CorruptBlobOnDisk(h) => write!(f, "blob {h} failed SHA-256 integrity check"),
            Self::VersionNotFound(v) => write!(f, "version '{v}' not found in store"),
            Self::NotAnArtifactVersion(v) => write!(f, "version '{v}' is not an Artifact"),
            Self::DuplicateJobId(id) => write!(f, "duplicate job ID '{id}'"),
            Self::JobNotFound(id) => write!(f, "job '{id}' not found"),
            Self::InvalidJobTransition { job_id, current } => {
                write!(
                    f,
                    "invalid transition for job '{job_id}' in state {current:?}"
                )
            }
            Self::StaleOrRevokedLease {
                job_id,
                attempted_epoch,
            } => write!(
                f,
                "rejected late publication for job '{job_id}' with stale/revoked lease epoch {attempted_epoch}"
            ),
        }
    }
}

impl std::error::Error for StoreError {}

#[cfg(test)]
mod tests {
    use super::*;
    use oia_domain::{
        DomainObjectType, ObjectEnvelope, OriginClass, ProvenanceRecord, ReassessmentState,
        RetainedContent, ReviewStatus, SchemaVersion, WorkspaceVisibility,
    };

    fn temp_store_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("oia_store_test_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        p
    }

    /// Verifies atomic source/artifact commit, disk persistence across `EvidenceStore::open`,
    /// multi-artifact shared-hash tombstoning, pre-migration backup purge on tombstone,
    /// backup/restore integrity, and stale job lease rejection (`#11`, `#14`, `A1`–`A3`, `A01`, `A17`).
    #[test]
    fn transactional_store_tombstones_backup_and_lease_revocation()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = temp_store_dir("p0_store");
        let restore_dir = temp_store_dir("p0_restore");

        // 1. Future schema is rejected cleanly.
        assert!(matches!(
            EvidenceStore::open(&dir, 99, StoreBudget::default()),
            Err(StoreError::UnsupportedFutureSchema { found: 99, .. })
        ));

        let mut store = EvidenceStore::open(
            &dir,
            1,
            StoreBudget {
                max_total_blob_bytes: 1024,
                max_single_blob_bytes: 512,
            },
        )?;

        let actor = ActorId::new("analyst")?;
        let payload = b"original declassified memo bytes";
        let hash = ContentHash::compute(payload);

        let make_sr = |id: &str, ver: &str, h: ContentHash| -> Result<SourceRecord, DomainError> {
            Ok(SourceRecord {
                envelope: ObjectEnvelope {
                    id: OpaqueId::new(id)?,
                    version_id: VersionId::new(ver)?,
                    schema_version: SchemaVersion::V1,
                    object_type: DomainObjectType::SourceRecord,
                    created_at_ms: 1_700_000_000_000,
                    created_by: actor.clone(),
                    visibility: WorkspaceVisibility::PrivateCase {
                        case_id: OpaqueId::new("case_1")?,
                        sensitive: false,
                    },
                    supersedes: None,
                    provenance: ProvenanceRecord {
                        source_version_ids: Vec::new(),
                        declared_origin_note: Some("Archive import".to_owned()),
                        processing_history: Vec::new(),
                    },
                    origin_class: OriginClass::AcquiredSourceMaterial,
                    review_status: ReviewStatus::Unreviewed,
                    availability: AvailabilityState::Available,
                    reassessment: ReassessmentState::Current,
                },
                source_id: OpaqueId::new("src_nara")?,
                upstream_version: "1976-A".to_owned(),
                acquisition_attempt_id: OpaqueId::new("acq_memo_1")?,
                source_locator: "https://archives.example/memo.txt".to_owned(),
                source_event_time_ms: None,
                public_availability_time_ms: Some(1_600_000_000_000),
                retrieved_at_ms: 1_700_000_000_000,
                content: RetainedContent::RetainedBlob {
                    content_hash: h,
                    byte_len: payload.len() as u64,
                    media_type: "text/plain".to_owned(),
                },
            })
        };

        let art_ver_1 = VersionId::new("ver_art_memo_1")?;
        let art_ver_2 = VersionId::new("ver_art_memo_2")?;
        let make_art = |id: &str,
                        ver: &VersionId,
                        sr_ver: &str,
                        h: ContentHash|
         -> Result<Artifact, DomainError> {
            Ok(Artifact {
                envelope: ObjectEnvelope {
                    id: OpaqueId::new(id)?,
                    version_id: ver.clone(),
                    schema_version: SchemaVersion::V1,
                    object_type: DomainObjectType::Artifact,
                    created_at_ms: 1_700_000_000_000,
                    created_by: actor.clone(),
                    visibility: WorkspaceVisibility::PrivateCase {
                        case_id: OpaqueId::new("case_1")?,
                        sensitive: false,
                    },
                    supersedes: None,
                    provenance: ProvenanceRecord {
                        source_version_ids: vec![VersionId::new(sr_ver)?],
                        declared_origin_note: None,
                        processing_history: Vec::new(),
                    },
                    origin_class: OriginClass::AcquiredSourceMaterial,
                    review_status: ReviewStatus::Unreviewed,
                    availability: AvailabilityState::Available,
                    reassessment: ReassessmentState::Current,
                },
                content_hash: h,
                media_type: "text/plain".to_owned(),
                byte_len: payload.len() as u64,
                derived_from_artifact: None,
                quality_effect_disclosure: None,
            })
        };

        // 2. Hash mismatch aborts commit and cleans staging directory without mutating store.
        let wrong_hash = ContentHash::compute(b"different bytes");
        let bad_commit = store.commit_source_and_artifact(
            make_sr("sr_memo_1", "ver_sr_memo_1", wrong_hash.clone())?,
            make_art("art_memo_1", &art_ver_1, "ver_sr_memo_1", wrong_hash)?,
            payload,
            RetentionClass::PinnedCaseEvidence,
        );
        assert!(matches!(bad_commit, Err(StoreError::HashMismatch { .. })));
        assert_eq!(store.total_blob_bytes(), 0);
        assert_eq!(store.cleanup_staging()?, 0);

        // 3. Valid commits succeed; two distinct artifacts sharing the same ContentHash are committed.
        store.commit_source_and_artifact(
            make_sr("sr_memo_1", "ver_sr_memo_1", hash.clone())?,
            make_art("art_memo_1", &art_ver_1, "ver_sr_memo_1", hash.clone())?,
            payload,
            RetentionClass::PinnedCaseEvidence,
        )?;
        store.commit_source_and_artifact(
            make_sr("sr_memo_2", "ver_sr_memo_2", hash.clone())?,
            make_art("art_memo_2", &art_ver_2, "ver_sr_memo_2", hash.clone())?,
            payload,
            RetentionClass::PinnedCaseEvidence,
        )?;
        assert_eq!(store.read_verified_blob(&hash)?, payload);

        // 4. Schema migration takes pre-migration backup snapshot and persists `schema_migrations.json`.
        store.migrate_schema(2, 1_700_000_001_000, "Upgrade to v2 job lease table")?;
        assert_eq!(store.schema_version(), 2);
        assert!(store.pre_migration_backup().is_some());

        // 5. Reopen store from `dir` (`A1`) and verify schema_version, objects, migrations, and blobs survive!
        let reopened = EvidenceStore::open(&dir, 1, StoreBudget::default())?;
        assert_eq!(reopened.schema_version(), 2);
        assert_eq!(reopened.migrations().len(), 1);
        assert!(reopened.graph().get_version(&art_ver_1).is_some());
        assert_eq!(reopened.read_verified_blob(&hash)?, payload);
        drop(reopened);

        // 6. Backup and restore verify SHA-256 integrity and reject corrupted backups.
        let mut backup = store.create_backup(1_700_000_002_000)?;
        let restored =
            EvidenceStore::restore_from_backup(&restore_dir, &backup, StoreBudget::default())?;
        assert_eq!(restored.read_verified_blob(&hash)?, payload);

        if let Some(bytes) = backup.blobs.get_mut(&hash) {
            bytes[0] ^= 0xff;
        }
        let corrupt_restore_dir = temp_store_dir("p0_corrupt_restore");
        assert!(matches!(
            EvidenceStore::restore_from_backup(
                &corrupt_restore_dir,
                &backup,
                StoreBudget::default()
            ),
            Err(StoreError::CorruptBlobOnDisk(_))
        ));

        // 7. Durable job lease revocation prevents superseded/cancelled worker from publishing late outputs (A01).
        let job_old = OpaqueId::new("job_acq_old")?;
        let job_new = OpaqueId::new("job_acq_new")?;
        store.enqueue_job(job_old.clone(), None, "source_ingest", Vec::new(), 2)?;
        let old_lease = store.acquire_job_lease(&job_old, "worker_1", 1_700_000_010_000)?;
        store.supersede_job(&job_old, job_new)?;

        let late_publish = store.complete_job_with_outputs(
            &job_old,
            old_lease.lease_epoch,
            1_700_000_005_000,
            None,
            Vec::new(),
        );
        assert!(matches!(
            late_publish,
            Err(StoreError::StaleOrRevokedLease { .. })
        ));

        // 8. Tombstoning first artifact keeps shared blob alive for `art_ver_2`, then tombstoning
        //    `art_ver_2` deletes bytes from disk AND purges `pre_migration_backup` (`A3`).
        store.tombstone_artifact(
            &art_ver_1,
            actor.clone(),
            1_700_000_020_000,
            "First publisher release revoked",
        )?;
        assert_eq!(
            store
                .graph()
                .get_version(&art_ver_1)
                .map(|o| o.envelope().availability),
            Some(AvailabilityState::Removed)
        );
        // `art_ver_2` is still Available, so blob is still readable.
        assert_eq!(store.read_verified_blob(&hash)?, payload);

        store.tombstone_artifact(
            &art_ver_2,
            actor,
            1_700_000_021_000,
            "Second publisher release revoked",
        )?;
        assert_eq!(
            store.read_verified_blob(&hash),
            Err(StoreError::BlobTombstoned(hash.clone()))
        );
        // Verify pre_migration_backup no longer retains the tombstoned blob bytes in memory (`A3`).
        assert!(
            store
                .pre_migration_backup()
                .is_none_or(|b| !b.blobs.contains_key(&hash))
        );

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&restore_dir);
        let _ = fs::remove_dir_all(&corrupt_restore_dir);
        Ok(())
    }
}
