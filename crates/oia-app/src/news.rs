use crate::local_tools::{Control, Result, ToolError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub const SCHEMA: &str = "oia-news-dossier/1";
pub const MAX_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_VERSIONS: usize = 200;
pub const EXAMPLE: &[u8] = include_bytes!("../examples/news-dossier.json");
const MAX_TEXT_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportKind {
    Plan,
    ReportedObservation,
    Allegation,
    Arrest,
    Charge,
    Judgment,
    AttributedStatement,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationState {
    Published,
    Corrected,
    Retracted,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionCoverage {
    Complete,
    Partial,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Article {
    pub record_id: String,
    pub version_id: String,
    pub source_id: String,
    pub publisher: String,
    pub title: String,
    pub url: Option<String>,
    pub region: Option<String>,
    pub origin_id: Option<String>,
    pub event_id: Option<String>,
    pub kind: ReportKind,
    pub publication_state: PublicationState,
    pub published_time: Option<String>,
    pub updated_time: Option<String>,
    pub event_time: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    schema: String,
    title: String,
    synthetic: bool,
    collection_coverage: CollectionCoverage,
    coverage_note: String,
    articles: Vec<Article>,
}

#[derive(Debug)]
pub struct Dossier {
    input: Input,
    original: Vec<u8>,
    sha256: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    pub query: String,
    pub region: Option<String>,
    pub kind: Option<ReportKind>,
}

#[derive(Debug, Serialize)]
pub struct Counts {
    pub versions: usize,
    pub records: usize,
    pub sources: usize,
    pub declared_origins: usize,
    pub unknown_origin_versions: usize,
    pub proposed_events: usize,
    pub ungrouped_versions: usize,
    pub missing_text_versions: usize,
}

#[derive(Debug, Serialize)]
pub struct ArticleView {
    #[serde(flatten)]
    pub article: Article,
    pub text_sha256: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SourceView {
    pub source_id: String,
    pub publisher: String,
    pub counts: Counts,
}

#[derive(Debug, Serialize)]
pub struct View {
    pub title: String,
    pub synthetic: bool,
    pub sha256: String,
    pub original_bytes: usize,
    pub collection_coverage: CollectionCoverage,
    pub coverage_note: String,
    pub total: Counts,
    pub visible: Counts,
    pub regions: Vec<String>,
    pub source_roster: Vec<SourceView>,
    pub articles: Vec<ArticleView>,
}

fn invalid(message: impl ToString) -> ToolError {
    ToolError::new("invalid_news", message)
}

fn text_field(value: &str, field: &str, limit: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > limit || value.contains('\0') {
        return Err(invalid(format!(
            "{field} must contain text, have no NUL, and fit {limit} UTF-8 bytes."
        )));
    }
    Ok(())
}

fn identifier(value: &str, field: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:/@".contains(&byte))
    {
        return Err(invalid(format!(
            "{field} must use 1-128 ASCII letters, digits, or -_.:/@."
        )));
    }
    Ok(())
}

fn counts(articles: &[&Article]) -> Counts {
    Counts {
        versions: articles.len(),
        records: articles
            .iter()
            .map(|article| &article.record_id)
            .collect::<BTreeSet<_>>()
            .len(),
        sources: articles
            .iter()
            .map(|article| &article.source_id)
            .collect::<BTreeSet<_>>()
            .len(),
        declared_origins: articles
            .iter()
            .filter_map(|article| article.origin_id.as_ref())
            .collect::<BTreeSet<_>>()
            .len(),
        unknown_origin_versions: articles
            .iter()
            .filter(|article| article.origin_id.is_none())
            .count(),
        proposed_events: articles
            .iter()
            .filter_map(|article| article.event_id.as_ref())
            .collect::<BTreeSet<_>>()
            .len(),
        ungrouped_versions: articles
            .iter()
            .filter(|article| article.event_id.is_none())
            .count(),
        missing_text_versions: articles
            .iter()
            .filter(|article| article.text.is_none())
            .count(),
    }
}

impl Dossier {
    pub fn parse(bytes: &[u8], control: &Control) -> Result<Self> {
        control.check()?;
        if bytes.len() > MAX_BYTES {
            return Err(invalid("A news dossier may not exceed 2 MiB."));
        }
        let input: Input = serde_json::from_slice(bytes)
            .map_err(|error| invalid(format!("Invalid news dossier JSON: {error}")))?;
        if input.schema != SCHEMA {
            return Err(invalid(format!("Expected schema {SCHEMA}.")));
        }
        text_field(&input.title, "title", 512)?;
        text_field(&input.coverage_note, "coverage_note", 2048)?;
        if input.articles.len() > MAX_VERSIONS {
            return Err(invalid(
                "A dossier may contain at most 200 article versions.",
            ));
        }
        let mut versions = BTreeSet::new();
        let mut records = BTreeMap::new();
        let mut publishers = BTreeMap::new();
        for article in &input.articles {
            control.check()?;
            identifier(&article.record_id, "record_id")?;
            identifier(&article.version_id, "version_id")?;
            identifier(&article.source_id, "source_id")?;
            if !versions.insert(&article.version_id) {
                return Err(invalid(
                    "Duplicate version_id; retain each exact version only once.",
                ));
            }
            if records
                .insert(&article.record_id, &article.source_id)
                .is_some_and(|source| source != &article.source_id)
            {
                return Err(invalid(
                    "Versions of a record must keep the same source_id.",
                ));
            }
            text_field(&article.publisher, "publisher", 512)?;
            if publishers
                .insert(&article.source_id, &article.publisher)
                .is_some_and(|name| name != &article.publisher)
            {
                return Err(invalid(
                    "A source_id must have one consistent publisher label.",
                ));
            }
            text_field(&article.title, "article title", 512)?;
            for (field, value) in [
                ("origin_id", &article.origin_id),
                ("event_id", &article.event_id),
            ] {
                if let Some(value) = value {
                    identifier(value, field)?;
                }
            }
            for (field, value, limit) in [
                ("url", &article.url, 2048),
                ("region", &article.region, 512),
                ("published_time", &article.published_time, 160),
                ("updated_time", &article.updated_time, 160),
                ("event_time", &article.event_time, 160),
                ("text", &article.text, MAX_TEXT_BYTES),
            ] {
                if let Some(value) = value {
                    text_field(value, field, limit)?;
                }
            }
        }
        control.check()?;
        Ok(Self {
            input,
            original: bytes.to_vec(),
            sha256: format!("{:x}", Sha256::digest(bytes)),
        })
    }

    pub fn open(path: &Path, control: &Control) -> Result<Self> {
        control.check()?;
        let file = File::open(path).map_err(|error| ToolError::new("read_news", error))?;
        let metadata = file
            .metadata()
            .map_err(|error| ToolError::new("read_news", error))?;
        if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
            return Err(invalid("Select a regular JSON file of at most 2 MiB."));
        }
        let mut bytes = Vec::new();
        file.take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| ToolError::new("read_news", error))?;
        Self::parse(&bytes, control)
    }

    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    #[must_use]
    pub fn original_bytes(&self) -> &[u8] {
        &self.original
    }

    pub fn view(&self, filter: &Filter, control: &Control) -> Result<View> {
        control.check()?;
        if filter.query.len() > 256 {
            return Err(invalid("News search is limited to 256 UTF-8 bytes."));
        }
        if let Some(region) = &filter.region {
            text_field(region, "region filter", 512)?;
        }
        let all: Vec<_> = self.input.articles.iter().collect();
        let mut matching = Vec::new();
        for article in &self.input.articles {
            control.check()?;
            if filter
                .region
                .as_ref()
                .is_some_and(|region| article.region.as_ref() != Some(region))
                || filter.kind.is_some_and(|kind| article.kind != kind)
            {
                continue;
            }
            if !filter.query.is_empty()
                && ![
                    Some(article.title.as_str()),
                    Some(article.publisher.as_str()),
                    article.text.as_deref(),
                ]
                .into_iter()
                .flatten()
                .any(|text| text.contains(&filter.query))
            {
                continue;
            }
            matching.push(article);
        }
        let mut sources: BTreeMap<&str, (&str, Vec<&Article>)> = BTreeMap::new();
        for article in &matching {
            sources
                .entry(&article.source_id)
                .or_insert_with(|| (&article.publisher, Vec::new()))
                .1
                .push(article);
        }
        let source_roster = sources
            .into_iter()
            .map(|(source_id, (publisher, articles))| SourceView {
                source_id: source_id.to_string(),
                publisher: publisher.to_string(),
                counts: counts(&articles),
            })
            .collect();
        let view = View {
            title: self.input.title.clone(),
            synthetic: self.input.synthetic,
            sha256: self.sha256.clone(),
            original_bytes: self.original.len(),
            collection_coverage: self.input.collection_coverage,
            coverage_note: self.input.coverage_note.clone(),
            total: counts(&all),
            visible: counts(&matching),
            source_roster,
            regions: all
                .iter()
                .filter_map(|article| article.region.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            articles: matching
                .into_iter()
                .map(|article| ArticleView {
                    article: article.clone(),
                    text_sha256: article
                        .text
                        .as_ref()
                        .map(|text| format!("{:x}", Sha256::digest(text.as_bytes()))),
                })
                .collect(),
        };
        control.check()?;
        Ok(view)
    }
}
