//! Job tray model (design §5.3): states, reported-only progress, partial
//! extent, cancel/retry, supersession of results for stale queries, and
//! interruption on restart. No window or IPC dependency.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    PartiallySucceeded,
    Failed,
    Cancelled,
    Superseded,
    Interrupted,
}

impl JobState {
    #[must_use]
    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }

    #[must_use]
    pub fn can_retry(self) -> bool {
        matches!(self, Self::Failed | Self::Cancelled | Self::Interrupted)
    }

    #[must_use]
    pub fn has_result(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::PartiallySucceeded | Self::Superseded
        )
    }
}

/// Progress as reported by a worker, with units. Absent progress is shown as
/// "progress not reported", never as a made-up percentage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub done: u32,
    pub total: u32,
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobKind {
    /// Fixture OCR worker over a fixture document.
    FixtureOcr { document: String },
    /// Fixture acquisition that always fails with an unavailable provider.
    FixtureAcquire { release: String },
    /// Exact-text search over fixture passages, tied to a query sequence.
    Search {
        query: String,
        scope: String,
        query_seq: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub name: String,
    pub target: String,
    pub kind: JobKind,
    pub state: JobState,
    pub progress: Option<Progress>,
    /// Statement of partial extent, carried with the result (design §5.3).
    pub extent: Option<String>,
    pub error: Option<String>,
    pub attempt: u32,
    pub max_attempts: u32,
    /// Retained result, including for superseded jobs.
    pub result: Option<serde_json::Value>,
    /// Whether the result has been applied to the current view. Superseded
    /// results stay `false` until the user opens them.
    pub applied: bool,
    pub updated_at_unix: u64,
    /// Always true in the spike: no job here does real work.
    pub fixture: bool,
}

/// How a worker reports completion.
#[derive(Debug, Clone, PartialEq)]
pub enum Finish {
    Succeeded {
        result: serde_json::Value,
    },
    PartiallySucceeded {
        result: serde_json::Value,
        extent: String,
    },
    Failed {
        error: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobError {
    UnknownJob(String),
    NotActive { id: String, state: JobState },
    NotRetryable { id: String, state: JobState },
    AttemptsExhausted { id: String, max_attempts: u32 },
    NoResult { id: String, state: JobState },
}

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownJob(id) => write!(f, "unknown job {id}"),
            Self::NotActive { id, state } => write!(f, "job {id} is not active ({state:?})"),
            Self::NotRetryable { id, state } => write!(f, "job {id} cannot be retried ({state:?})"),
            Self::AttemptsExhausted { id, max_attempts } => {
                write!(f, "job {id} has used all {max_attempts} attempts")
            }
            Self::NoResult { id, state } => write!(f, "job {id} has no result ({state:?})"),
        }
    }
}

impl std::error::Error for JobError {}

/// Upper bound on retained job records; finished jobs are dropped oldest first.
pub const JOB_LIMIT: usize = 100;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobRegistry {
    next_id: u64,
    jobs: Vec<Job>,
}

impl Default for JobRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl JobRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_id: 1,
            jobs: Vec::new(),
        }
    }

    #[must_use]
    pub fn jobs(&self) -> &[Job] {
        &self.jobs
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Job> {
        self.jobs.iter().find(|j| j.id == id)
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut Job, JobError> {
        self.jobs
            .iter_mut()
            .find(|j| j.id == id)
            .ok_or_else(|| JobError::UnknownJob(id.to_owned()))
    }

    fn evict_if_needed(&mut self) {
        while self.jobs.len() >= JOB_LIMIT {
            match self.jobs.iter().position(|j| !j.state.is_active()) {
                Some(index) => {
                    self.jobs.remove(index);
                }
                None => break,
            }
        }
    }

    /// Registers a job in the given initial state and returns its id.
    pub fn start(
        &mut self,
        name: &str,
        target: &str,
        kind: JobKind,
        state: JobState,
        progress: Option<Progress>,
        max_attempts: u32,
    ) -> String {
        self.evict_if_needed();
        let id = format!("job-{}", self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.jobs.push(Job {
            id: id.clone(),
            name: name.to_owned(),
            target: target.to_owned(),
            kind,
            state,
            progress,
            extent: None,
            error: None,
            attempt: 1,
            max_attempts,
            result: None,
            applied: false,
            updated_at_unix: now_unix(),
            fixture: true,
        });
        id
    }

    pub fn set_running(&mut self, id: &str) -> Result<(), JobError> {
        let job = self.get_mut(id)?;
        if job.state != JobState::Queued {
            return Err(JobError::NotActive {
                id: id.to_owned(),
                state: job.state,
            });
        }
        job.state = JobState::Running;
        job.updated_at_unix = now_unix();
        Ok(())
    }

    /// Records worker-reported progress for a running job.
    pub fn report_progress(&mut self, id: &str, progress: Progress) -> Result<(), JobError> {
        let job = self.get_mut(id)?;
        if job.state != JobState::Running {
            return Err(JobError::NotActive {
                id: id.to_owned(),
                state: job.state,
            });
        }
        job.progress = Some(progress);
        job.updated_at_unix = now_unix();
        Ok(())
    }

    /// Completes a job. A search whose query sequence is no longer current is
    /// marked superseded: its result is retained but `applied` stays false.
    pub fn finish(
        &mut self,
        id: &str,
        finish: Finish,
        current_query_seq: u64,
    ) -> Result<JobState, JobError> {
        let job = self.get_mut(id)?;
        if !job.state.is_active() {
            return Err(JobError::NotActive {
                id: id.to_owned(),
                state: job.state,
            });
        }
        let stale =
            matches!(job.kind, JobKind::Search { query_seq, .. } if query_seq != current_query_seq);
        match finish {
            Finish::Succeeded { result } => {
                job.result = Some(result);
                job.extent = None;
                job.error = None;
                if stale {
                    job.state = JobState::Superseded;
                    job.applied = false;
                } else {
                    job.state = JobState::Succeeded;
                    job.applied = true;
                }
            }
            Finish::PartiallySucceeded { result, extent } => {
                job.result = Some(result);
                job.extent = Some(extent);
                job.error = None;
                if stale {
                    job.state = JobState::Superseded;
                    job.applied = false;
                } else {
                    job.state = JobState::PartiallySucceeded;
                    job.applied = true;
                }
            }
            Finish::Failed { error } => {
                job.error = Some(error);
                job.state = JobState::Failed;
                job.applied = false;
            }
        }
        job.updated_at_unix = now_unix();
        Ok(job.state)
    }

    pub fn cancel(&mut self, id: &str) -> Result<(), JobError> {
        let job = self.get_mut(id)?;
        if !job.state.is_active() {
            return Err(JobError::NotActive {
                id: id.to_owned(),
                state: job.state,
            });
        }
        job.state = JobState::Cancelled;
        job.updated_at_unix = now_unix();
        Ok(())
    }

    pub fn retry(&mut self, id: &str) -> Result<(), JobError> {
        let job = self.get_mut(id)?;
        if !job.state.can_retry() {
            return Err(JobError::NotRetryable {
                id: id.to_owned(),
                state: job.state,
            });
        }
        if job.attempt >= job.max_attempts {
            return Err(JobError::AttemptsExhausted {
                id: id.to_owned(),
                max_attempts: job.max_attempts,
            });
        }
        job.attempt = job.attempt.saturating_add(1);
        job.state = JobState::Queued;
        job.error = None;
        job.updated_at_unix = now_unix();
        Ok(())
    }

    /// The user explicitly opens a retained result; only then may it apply.
    pub fn mark_applied(&mut self, id: &str) -> Result<serde_json::Value, JobError> {
        let job = self.get_mut(id)?;
        if !job.state.has_result() {
            return Err(JobError::NoResult {
                id: id.to_owned(),
                state: job.state,
            });
        }
        job.applied = true;
        job.updated_at_unix = now_unix();
        Ok(job.result.clone().unwrap_or(serde_json::Value::Null))
    }

    /// On restore, nothing that was active can be trusted to have finished.
    pub fn mark_interrupted(&mut self) -> u32 {
        let mut count = 0;
        for job in &mut self.jobs {
            if job.state.is_active() {
                job.state = JobState::Interrupted;
                job.updated_at_unix = now_unix();
                count += 1;
            }
        }
        count
    }

    /// Ids of jobs in a given state, oldest first.
    #[must_use]
    pub fn ids_in_state(&self, state: JobState) -> Vec<String> {
        self.jobs
            .iter()
            .filter(|j| j.state == state)
            .map(|j| j.id.clone())
            .collect()
    }
}
