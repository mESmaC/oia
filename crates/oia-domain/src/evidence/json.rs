//! Strict JSON form of an [`EvidenceLedger`] (ADR 0006 / D05 round-trip format).
//!
//! The document lists accepted operations in order. Import replays each entry through
//! [`EvidenceLedger::issue`], [`EvidenceLedger::revise`] or [`EvidenceLedger::relate`], so
//! imported data passes the same validation as live input. Lineage labels and reassessment
//! flags are derived state: they are recomputed on import and are not trusted from the file.
//!
//! Unknown fields, duplicate fields, unknown codes and malformed IDs are rejected. Integers are
//! exact `i64`; consumers limited to IEEE doubles must not round them.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use super::ledger::PropagationBudget;
use super::{
    AttributeName, AttributeValue, Availability, ClaimAssessment, ClaimFields, EvidenceError,
    EvidenceLedger, FieldValue, LedgerEntry, LedgerLimits, ObjectType, OriginClass, Provenance,
    RecordFields, RecordVersion, Relation, RelationKind, ReviewStatus, Revision, RevisionReason,
    Scope, UnknownReason,
};
use crate::{ActorId, IdError, OpaqueId, VersionId};

/// Format identifier written in every ledger document.
pub const LEDGER_FORMAT: &str = "oia.evidence-ledger";
/// Largest accepted JSON document, checked before parsing.
pub const MAX_LEDGER_JSON_BYTES: usize = 64 * 1024 * 1024;
const LEDGER_SCHEMA_VERSION: u32 = 1;

/// Import and export failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    Oversized {
        len: usize,
        max: usize,
    },
    /// Malformed JSON, an unknown or duplicate field, or a wrong value type.
    Syntax(String),
    UnsupportedFormat(String),
    UnsupportedSchema(u32),
    UnknownCode {
        entry: usize,
        field: &'static str,
        code: String,
    },
    InvalidId {
        entry: usize,
        field: &'static str,
        source: IdError,
    },
    /// The entry was well-formed but the ledger rejected it.
    Rejected {
        entry: usize,
        source: EvidenceError,
    },
    Serialize(String),
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Oversized { len, max } => {
                write!(f, "ledger JSON is {len} bytes; the limit is {max}")
            }
            Self::Syntax(message) => write!(f, "invalid ledger JSON: {message}"),
            Self::UnsupportedFormat(format) => write!(f, "unsupported format {format:?}"),
            Self::UnsupportedSchema(v) => write!(f, "unsupported schema version {v}"),
            Self::UnknownCode { entry, field, code } => {
                write!(f, "entry {entry}: unknown {field} code {code:?}")
            }
            Self::InvalidId {
                entry,
                field,
                source,
            } => write!(f, "entry {entry}: invalid {field}: {source}"),
            Self::Rejected { entry, source } => write!(f, "entry {entry}: {source}"),
            Self::Serialize(message) => write!(f, "could not serialize ledger: {message}"),
        }
    }
}

impl std::error::Error for CodecError {}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LedgerDoc {
    format: String,
    schema_version: u32,
    entries: Vec<EntryDoc>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum EntryDoc {
    Record(Box<RecordDoc>),
    Relation(RelationDoc),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordDoc {
    id: String,
    version: String,
    schema_version: u32,
    object_type: String,
    origin: String,
    created_at_ms: i64,
    created_by: String,
    scope: ScopeDoc,
    access_policies: Vec<String>,
    provenance: ProvenanceDoc,
    review: String,
    availability: String,
    predecessor: Option<String>,
    revision: Option<RevisionDoc>,
    attributes: Vec<AttributeDoc>,
    claim: Option<ClaimDoc>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum ScopeDoc {
    Case(String),
    Library,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum ProvenanceDoc {
    Acquired {
        source: String,
    },
    Manual,
    Derived {
        method: String,
        method_version: String,
        inputs: Vec<String>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionDoc {
    reason: String,
    rationale: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttributeDoc {
    name: String,
    value: FieldDoc<AttributeValueDoc>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum FieldDoc<T> {
    Known(T),
    Unknown(String),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum AttributeValueDoc {
    Text(String),
    Integer(i64),
    Boolean(bool),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClaimDoc {
    proposition: String,
    claimant: FieldDoc<String>,
    assessment: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RelationDoc {
    id: String,
    kind: String,
    dependent: String,
    upstream: String,
    created_at_ms: i64,
    created_by: String,
    review: String,
    evidence: Vec<String>,
    rationale: FieldDoc<String>,
}

impl EvidenceLedger {
    /// Serializes accepted operations in order as pretty-printed JSON with a trailing newline.
    /// Output is deterministic: attributes are sorted by name, and other lists keep their
    /// recorded order.
    pub fn to_json(&self) -> Result<String, CodecError> {
        let mut entries = Vec::with_capacity(self.entries().len());
        for entry in self.entries() {
            let doc = match entry {
                LedgerEntry::Record(version) => {
                    let record = self.version(version).ok_or_else(|| {
                        CodecError::Serialize(format!("missing version {version}"))
                    })?;
                    EntryDoc::Record(Box::new(record_doc(record)))
                }
                LedgerEntry::Relation(id) => {
                    let relation = self
                        .relation(id)
                        .ok_or_else(|| CodecError::Serialize(format!("missing relation {id}")))?;
                    EntryDoc::Relation(relation_doc(relation))
                }
            };
            entries.push(doc);
        }
        let doc = LedgerDoc {
            format: LEDGER_FORMAT.to_owned(),
            schema_version: LEDGER_SCHEMA_VERSION,
            entries,
        };
        let mut json =
            serde_json::to_string_pretty(&doc).map_err(|e| CodecError::Serialize(e.to_string()))?;
        json.push('\n');
        Ok(json)
    }

    /// Parses and replays a ledger document under `limits`.
    ///
    /// Inputs larger than [`MAX_LEDGER_JSON_BYTES`] are refused before parsing. Import stops at
    /// the first rejected entry and returns no partial ledger. Each operation obeys
    /// [`LedgerLimits::max_propagation_steps`], and all replayed operations together obey
    /// [`LedgerLimits::max_import_propagation_steps`]. Work limits never truncate flags.
    pub fn from_json(input: &str, limits: LedgerLimits) -> Result<Self, CodecError> {
        if input.len() > MAX_LEDGER_JSON_BYTES {
            return Err(CodecError::Oversized {
                len: input.len(),
                max: MAX_LEDGER_JSON_BYTES,
            });
        }
        let doc: LedgerDoc =
            serde_json::from_str(input).map_err(|e| CodecError::Syntax(e.to_string()))?;
        let tree: serde_json::Value =
            serde_json::from_str(input).map_err(|e| CodecError::Syntax(e.to_string()))?;
        shape::document(&tree)?;
        if doc.format != LEDGER_FORMAT {
            return Err(CodecError::UnsupportedFormat(doc.format));
        }
        if doc.schema_version != LEDGER_SCHEMA_VERSION {
            return Err(CodecError::UnsupportedSchema(doc.schema_version));
        }
        let mut ledger = Self::new(limits);
        let mut work = PropagationBudget::for_import(limits);
        for (index, entry) in doc.entries.into_iter().enumerate() {
            let rejected = |source| CodecError::Rejected {
                entry: index,
                source,
            };
            match entry {
                EntryDoc::Record(record) => {
                    if record.schema_version != LEDGER_SCHEMA_VERSION {
                        return Err(CodecError::UnsupportedSchema(record.schema_version));
                    }
                    let (fields, predecessor, revision) = record_from_doc(index, *record)?;
                    match (predecessor, revision) {
                        (None, None) => ledger
                            .issue_with_budget(fields, &mut work)
                            .map_err(rejected)?,
                        (Some(predecessor), Some(revision)) => {
                            ledger
                                .revise_with_budget(fields, &predecessor, revision, &mut work)
                                .map_err(rejected)?;
                        }
                        (None, Some(_)) | (Some(_), None) => {
                            return Err(CodecError::Syntax(format!(
                                "entry {index}: predecessor and revision must both be present or both be null"
                            )));
                        }
                    }
                }
                EntryDoc::Relation(relation) => {
                    let relation = relation_from_doc(index, relation)?;
                    ledger
                        .relate_with_budget(relation, &mut work)
                        .map_err(rejected)?;
                }
            }
        }
        Ok(ledger)
    }
}

fn record_doc(record: &RecordVersion) -> RecordDoc {
    let f = record.fields();
    RecordDoc {
        id: f.id.to_string(),
        version: f.version.to_string(),
        schema_version: record.schema_version().as_u32(),
        object_type: f.object_type.as_str().to_owned(),
        origin: f.origin.as_str().to_owned(),
        created_at_ms: f.created_at_ms,
        created_by: f.created_by.to_string(),
        scope: match &f.scope {
            Scope::Case(case) => ScopeDoc::Case(case.to_string()),
            Scope::Library => ScopeDoc::Library,
        },
        access_policies: f.access_policies.iter().map(ToString::to_string).collect(),
        provenance: match &f.provenance {
            Provenance::Acquired { source } => ProvenanceDoc::Acquired {
                source: source.to_string(),
            },
            Provenance::Manual => ProvenanceDoc::Manual,
            Provenance::Derived {
                method,
                method_version,
                inputs,
            } => ProvenanceDoc::Derived {
                method: method.to_string(),
                method_version: method_version.to_string(),
                inputs: inputs.iter().map(ToString::to_string).collect(),
            },
        },
        review: f.review.as_str().to_owned(),
        availability: f.availability.as_str().to_owned(),
        predecessor: record.predecessor().map(ToString::to_string),
        revision: record.revision().map(|r| RevisionDoc {
            reason: r.reason.as_str().to_owned(),
            rationale: r.rationale.clone(),
        }),
        attributes: f
            .attributes
            .iter()
            .map(|(name, value)| AttributeDoc {
                name: name.to_string(),
                value: field_doc(value, |v| match v {
                    AttributeValue::Text(t) => AttributeValueDoc::Text(t.clone()),
                    AttributeValue::Integer(i) => AttributeValueDoc::Integer(*i),
                    AttributeValue::Boolean(b) => AttributeValueDoc::Boolean(*b),
                }),
            })
            .collect(),
        claim: f.claim.as_ref().map(|c| ClaimDoc {
            proposition: c.proposition.clone(),
            claimant: field_doc(&c.claimant, Clone::clone),
            assessment: c.assessment.as_str().to_owned(),
        }),
    }
}

fn relation_doc(r: &Relation) -> RelationDoc {
    RelationDoc {
        id: r.id.to_string(),
        kind: r.kind.as_str().to_owned(),
        dependent: r.dependent.to_string(),
        upstream: r.upstream.to_string(),
        created_at_ms: r.created_at_ms,
        created_by: r.created_by.to_string(),
        review: r.review.as_str().to_owned(),
        evidence: r.evidence.iter().map(ToString::to_string).collect(),
        rationale: field_doc(&r.rationale, Clone::clone),
    }
}

fn field_doc<T, D>(value: &FieldValue<T>, map: impl Fn(&T) -> D) -> FieldDoc<D> {
    match value {
        FieldValue::Known(v) => FieldDoc::Known(map(v)),
        FieldValue::Unknown(reason) => FieldDoc::Unknown(reason.as_str().to_owned()),
    }
}

/// Converts one entry's raw strings into validated domain values.
struct Decoder {
    entry: usize,
}

impl Decoder {
    fn id(&self, field: &'static str, raw: String) -> Result<OpaqueId, CodecError> {
        OpaqueId::new(raw).map_err(|source| self.invalid(field, source))
    }

    fn version(&self, field: &'static str, raw: String) -> Result<VersionId, CodecError> {
        VersionId::new(raw).map_err(|source| self.invalid(field, source))
    }

    fn versions(
        &self,
        field: &'static str,
        raw: Vec<String>,
    ) -> Result<Vec<VersionId>, CodecError> {
        raw.into_iter().map(|v| self.version(field, v)).collect()
    }

    fn actor(&self, field: &'static str, raw: String) -> Result<ActorId, CodecError> {
        ActorId::new(raw).map_err(|source| self.invalid(field, source))
    }

    fn code<T>(
        &self,
        field: &'static str,
        raw: String,
        parse: fn(&str) -> Option<T>,
    ) -> Result<T, CodecError> {
        parse(&raw).ok_or(CodecError::UnknownCode {
            entry: self.entry,
            field,
            code: raw,
        })
    }

    fn field<T, D>(
        &self,
        doc: FieldDoc<D>,
        map: impl FnOnce(D) -> T,
    ) -> Result<FieldValue<T>, CodecError> {
        Ok(match doc {
            FieldDoc::Known(v) => FieldValue::Known(map(v)),
            FieldDoc::Unknown(code) => {
                FieldValue::Unknown(self.code("unknown_reason", code, UnknownReason::parse)?)
            }
        })
    }

    fn invalid(&self, field: &'static str, source: IdError) -> CodecError {
        CodecError::InvalidId {
            entry: self.entry,
            field,
            source,
        }
    }

    fn rejected(&self, source: EvidenceError) -> CodecError {
        CodecError::Rejected {
            entry: self.entry,
            source,
        }
    }
}

type DecodedRecord = (RecordFields, Option<VersionId>, Option<Revision>);

fn record_from_doc(entry: usize, doc: RecordDoc) -> Result<DecodedRecord, CodecError> {
    let d = Decoder { entry };
    let mut attributes = BTreeMap::new();
    let mut names = BTreeSet::new();
    for attribute in doc.attributes {
        let name = AttributeName::new(attribute.name).map_err(|e| d.rejected(e))?;
        if !names.insert(name.clone()) {
            return Err(d.rejected(EvidenceError::DuplicateAttribute(name)));
        }
        let value = d.field(attribute.value, |v| match v {
            AttributeValueDoc::Text(t) => AttributeValue::Text(t),
            AttributeValueDoc::Integer(i) => AttributeValue::Integer(i),
            AttributeValueDoc::Boolean(b) => AttributeValue::Boolean(b),
        })?;
        attributes.insert(name, value);
    }
    let claim = match doc.claim {
        Some(c) => Some(ClaimFields {
            proposition: c.proposition,
            claimant: d.field(c.claimant, |s| s)?,
            assessment: d.code("assessment", c.assessment, ClaimAssessment::parse)?,
        }),
        None => None,
    };
    let fields = RecordFields {
        id: d.id("id", doc.id)?,
        version: d.version("version", doc.version)?,
        object_type: d.code("object_type", doc.object_type, ObjectType::parse)?,
        origin: d.code("origin", doc.origin, OriginClass::parse)?,
        created_at_ms: doc.created_at_ms,
        created_by: d.actor("created_by", doc.created_by)?,
        scope: match doc.scope {
            ScopeDoc::Case(case) => Scope::Case(d.id("scope", case)?),
            ScopeDoc::Library => Scope::Library,
        },
        access_policies: doc
            .access_policies
            .into_iter()
            .map(|p| d.id("access_policies", p))
            .collect::<Result<_, _>>()?,
        provenance: match doc.provenance {
            ProvenanceDoc::Acquired { source } => Provenance::Acquired {
                source: d.id("source", source)?,
            },
            ProvenanceDoc::Manual => Provenance::Manual,
            ProvenanceDoc::Derived {
                method,
                method_version,
                inputs,
            } => Provenance::Derived {
                method: d.id("method", method)?,
                method_version: d.version("method_version", method_version)?,
                inputs: d.versions("inputs", inputs)?,
            },
        },
        review: d.code("review", doc.review, ReviewStatus::parse)?,
        availability: d.code("availability", doc.availability, Availability::parse)?,
        attributes,
        claim,
    };
    let predecessor = doc
        .predecessor
        .map(|p| d.version("predecessor", p))
        .transpose()?;
    let revision = match doc.revision {
        Some(r) => Some(Revision {
            reason: d.code("revision_reason", r.reason, RevisionReason::parse)?,
            rationale: r.rationale,
        }),
        None => None,
    };
    Ok((fields, predecessor, revision))
}

fn relation_from_doc(entry: usize, doc: RelationDoc) -> Result<Relation, CodecError> {
    let d = Decoder { entry };
    Ok(Relation {
        id: d.id("id", doc.id)?,
        kind: d.code("kind", doc.kind, RelationKind::parse)?,
        dependent: d.version("dependent", doc.dependent)?,
        upstream: d.version("upstream", doc.upstream)?,
        created_at_ms: doc.created_at_ms,
        created_by: d.actor("created_by", doc.created_by)?,
        review: d.code("review", doc.review, ReviewStatus::parse)?,
        evidence: d.versions("evidence", doc.evidence)?,
        rationale: d.field(doc.rationale, |s| s)?,
    })
}

/// Structural checks that serde's derived deserializers do not make: every struct must be a
/// JSON object (serde also accepts positional arrays), and every key, including nullable
/// ones, must be present (serde reads a missing `Option` as `null`). Field types, unknown
/// keys and duplicate keys are already checked by the typed parse.
mod shape {
    use serde_json::{Map, Value};

    use super::CodecError;

    type Obj = Map<String, Value>;

    fn fail(at: &str, message: &str) -> CodecError {
        CodecError::Syntax(format!("{at}: {message}"))
    }

    fn object<'a>(value: &'a Value, at: &str, keys: &[&str]) -> Result<&'a Obj, CodecError> {
        let Value::Object(map) = value else {
            return Err(fail(at, "expected a JSON object"));
        };
        for key in keys {
            if !map.contains_key(*key) {
                return Err(fail(at, &format!("missing field `{key}`")));
            }
        }
        Ok(map)
    }

    fn field<'a>(map: &'a Obj, key: &str) -> &'a Value {
        map.get(key).unwrap_or(&Value::Null)
    }

    /// An externally tagged enum value: a single-key object, or a bare string for unit
    /// variants listed in `units`.
    fn tagged<'a>(
        value: &'a Value,
        at: &str,
        units: &[&str],
    ) -> Result<Option<(&'a str, &'a Value)>, CodecError> {
        match value {
            Value::String(s) if units.contains(&s.as_str()) => Ok(None),
            Value::Object(map) if map.len() == 1 => {
                Ok(map.iter().next().map(|(k, v)| (k.as_str(), v)))
            }
            _ => Err(fail(at, "expected a single-key object")),
        }
    }

    fn array<'a>(value: &'a Value, at: &str) -> Result<&'a [Value], CodecError> {
        match value {
            Value::Array(items) => Ok(items),
            _ => Err(fail(at, "expected an array")),
        }
    }

    fn nullable(
        value: &Value,
        at: &str,
        check: impl FnOnce(&Value, &str) -> Result<(), CodecError>,
    ) -> Result<(), CodecError> {
        if value.is_null() {
            Ok(())
        } else {
            check(value, at)
        }
    }

    fn field_value(value: &Value, at: &str) -> Result<(), CodecError> {
        tagged(value, at, &[]).map(|_| ())
    }

    pub(super) fn document(value: &Value) -> Result<(), CodecError> {
        let doc = object(value, "document", &["format", "schema_version", "entries"])?;
        for (index, entry) in array(field(doc, "entries"), "entries")?.iter().enumerate() {
            let at = format!("entry {index}");
            match tagged(entry, &at, &[])? {
                Some(("record", body)) => record(body, &at)?,
                Some(("relation", body)) => relation(body, &at)?,
                _ => return Err(fail(&at, "expected `record` or `relation`")),
            }
        }
        Ok(())
    }

    fn record(value: &Value, at: &str) -> Result<(), CodecError> {
        let r = object(
            value,
            at,
            &[
                "id",
                "version",
                "schema_version",
                "object_type",
                "origin",
                "created_at_ms",
                "created_by",
                "scope",
                "access_policies",
                "provenance",
                "review",
                "availability",
                "predecessor",
                "revision",
                "attributes",
                "claim",
            ],
        )?;
        tagged(field(r, "scope"), at, &["library"])?;
        match tagged(field(r, "provenance"), at, &["manual"])? {
            Some(("acquired", body)) => {
                object(body, at, &["source"])?;
            }
            Some(("derived", body)) => {
                object(body, at, &["method", "method_version", "inputs"])?;
            }
            _ => {}
        }
        nullable(field(r, "revision"), at, |v, at| {
            object(v, at, &["reason", "rationale"]).map(|_| ())
        })?;
        for attribute in array(field(r, "attributes"), at)? {
            let a = object(attribute, at, &["name", "value"])?;
            let value = field(a, "value");
            field_value(value, at)?;
            if let Some(("known", inner)) = tagged(value, at, &[])? {
                field_value(inner, at)?;
            }
        }
        nullable(field(r, "claim"), at, |v, at| {
            let c = object(v, at, &["proposition", "claimant", "assessment"])?;
            field_value(field(c, "claimant"), at)
        })
    }

    fn relation(value: &Value, at: &str) -> Result<(), CodecError> {
        let r = object(
            value,
            at,
            &[
                "id",
                "kind",
                "dependent",
                "upstream",
                "created_at_ms",
                "created_by",
                "review",
                "evidence",
                "rationale",
            ],
        )?;
        field_value(field(r, "rationale"), at)
    }
}
