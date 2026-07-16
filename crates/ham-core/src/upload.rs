use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use ham_plugin_sdk::{
    OFFICIAL_LOG_UPLOAD_COMPLETED, OFFICIAL_LOG_UPLOAD_FAILED, OFFICIAL_LOG_UPLOAD_QUEUED,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    adif::export_adif,
    event::{CoreEventEnvelope, NewLogbookEvent},
    projection::{QsoCurrentStateProjection, QsoRecord},
    service::{LogUploadRequest, ServiceProviderMetadata},
    store::{LogbookEventStore, StoreError},
};

fn default_idempotency_key() -> String {
    Uuid::new_v4().to_string()
}

#[derive(Debug, Error)]
pub enum UploadQueueError {
    #[error("upload target {0} was not found")]
    TargetNotFound(String),
    #[error("upload provider {0} is missing required configuration: {1:?}")]
    MissingProviderConfig(String, Vec<String>),
    #[error("upload job {0} was not found")]
    JobNotFound(Uuid),
    #[error("upload queue is full")]
    QueueFull,
    #[error("upload job {0} is already claimed")]
    AlreadyClaimed(Uuid),
    #[error("upload job {0} claim token did not match")]
    ClaimTokenMismatch(Uuid),
    #[error("official upload event append failed: {0}")]
    Store(#[from] StoreError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadQueueState {
    Pending,
    Running,
    RetryScheduled,
    NeedsUserAction,
    Succeeded,
    Cancelled,
    DeadLetter,
    Uncertain,
}

impl UploadQueueState {
    pub fn is_claimable(self) -> bool {
        matches!(self, Self::Pending | Self::RetryScheduled | Self::Uncertain)
    }
}

fn default_queue_state() -> UploadQueueState {
    UploadQueueState::Pending
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadTarget {
    pub target_id: String,
    pub display_name: String,
    pub provider_id: String,
    pub enabled: bool,
    pub required_config_keys: Vec<String>,
}

impl UploadTarget {
    pub fn from_provider(provider: &ServiceProviderMetadata) -> Self {
        Self {
            target_id: provider.provider_id.clone(),
            display_name: provider.display_name.clone(),
            provider_id: provider.provider_id.clone(),
            enabled: true,
            required_config_keys: provider.required_config_keys.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadJobItem {
    pub qso_id: Uuid,
    pub status: UploadStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadJob {
    pub upload_job_id: Uuid,
    pub target_id: String,
    #[serde(default)]
    pub provider_id: String,
    #[serde(default)]
    pub account_scope: Option<String>,
    pub logbook_id: Uuid,
    #[serde(default)]
    pub operation_type: String,
    #[serde(default = "default_idempotency_key")]
    pub idempotency_key: String,
    pub qso_ids: Vec<Uuid>,
    pub items: Vec<UploadJobItem>,
    pub status: UploadStatus,
    #[serde(default = "default_queue_state")]
    pub queue_state: UploadQueueState,
    #[serde(default)]
    pub attempt_count: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub last_attempt_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    #[serde(default)]
    pub safe_failure_code: Option<String>,
    #[serde(default)]
    pub credential_reference: Option<Uuid>,
    #[serde(default)]
    pub provider_side_identifier: Option<String>,
    #[serde(default)]
    pub uncertain_outcome: bool,
    #[serde(default)]
    pub claim_token: Option<Uuid>,
    #[serde(default)]
    pub lease_expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadResult {
    pub upload_job_id: Uuid,
    pub target_id: String,
    pub status: UploadStatus,
    pub accepted_count: usize,
    pub failed_count: usize,
    pub message: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadQueue {
    pub targets: Vec<UploadTarget>,
    pub jobs: VecDeque<UploadJob>,
    #[serde(default = "default_queue_limit")]
    pub queue_limit: usize,
}

fn default_queue_limit() -> usize {
    1000
}

impl UploadQueue {
    pub fn new(targets: Vec<UploadTarget>) -> Self {
        Self {
            targets,
            jobs: VecDeque::new(),
            queue_limit: default_queue_limit(),
        }
    }

    pub fn create_job(
        &mut self,
        target_id: impl Into<String>,
        logbook_id: Uuid,
        qso_ids: Vec<Uuid>,
    ) -> Result<UploadJob, UploadQueueError> {
        let target_id = target_id.into();
        if !self
            .targets
            .iter()
            .any(|target| target.target_id == target_id)
        {
            return Err(UploadQueueError::TargetNotFound(target_id));
        }
        if self.jobs.len() >= self.queue_limit {
            return Err(UploadQueueError::QueueFull);
        }
        let now = Utc::now();
        let provider_id = self
            .targets
            .iter()
            .find(|target| target.target_id == target_id)
            .map(|target| target.provider_id.clone())
            .unwrap_or_else(|| target_id.clone());
        let idempotency_key = upload_idempotency_key(&provider_id, logbook_id, &qso_ids);
        let items = qso_ids
            .iter()
            .copied()
            .map(|qso_id| UploadJobItem {
                qso_id,
                status: UploadStatus::Queued,
                error: None,
            })
            .collect::<Vec<_>>();
        let job = UploadJob {
            upload_job_id: Uuid::new_v4(),
            target_id,
            provider_id,
            account_scope: None,
            logbook_id,
            operation_type: "upload.adif".to_owned(),
            idempotency_key,
            qso_ids,
            items,
            status: UploadStatus::Queued,
            queue_state: UploadQueueState::Pending,
            attempt_count: 0,
            created_at: now,
            updated_at: now,
            last_attempt_at: None,
            next_attempt_at: None,
            last_error: None,
            safe_failure_code: None,
            credential_reference: None,
            provider_side_identifier: None,
            uncertain_outcome: false,
            claim_token: None,
            lease_expires_at: None,
        };
        self.jobs.push_back(job.clone());
        Ok(job)
    }

    pub fn find_duplicate(&self, idempotency_key: &str) -> Option<&UploadJob> {
        self.jobs
            .iter()
            .find(|job| job.idempotency_key == idempotency_key)
    }

    pub fn claim_next(&mut self, now: DateTime<Utc>, lease_seconds: u64) -> Option<(Uuid, Uuid)> {
        let job = self.jobs.iter_mut().find(|job| {
            job.queue_state.is_claimable()
                && job.next_attempt_at.is_none_or(|next| next <= now)
                && job.lease_expires_at.is_none_or(|expires| expires <= now)
        })?;
        let token = Uuid::new_v4();
        job.status = UploadStatus::Running;
        job.queue_state = UploadQueueState::Running;
        job.attempt_count += 1;
        job.last_attempt_at = Some(now);
        job.updated_at = now;
        job.claim_token = Some(token);
        job.lease_expires_at = Some(now + chrono::Duration::seconds(lease_seconds as i64));
        Some((job.upload_job_id, token))
    }

    pub fn recover_expired_leases(&mut self, now: DateTime<Utc>) -> usize {
        let mut recovered = 0;
        for job in &mut self.jobs {
            if job.queue_state == UploadQueueState::Running
                && job.lease_expires_at.is_some_and(|expires| expires <= now)
            {
                job.status = UploadStatus::Failed;
                job.queue_state = UploadQueueState::Uncertain;
                job.uncertain_outcome = true;
                job.safe_failure_code = Some("worker_lease_expired".to_owned());
                job.last_error = Some("worker lease expired before completion".to_owned());
                job.claim_token = None;
                job.lease_expires_at = None;
                job.updated_at = now;
                recovered += 1;
            }
        }
        recovered
    }

    pub fn schedule_retry(
        &mut self,
        job_id: Uuid,
        claim_token: Uuid,
        next_attempt_at: DateTime<Utc>,
        safe_failure_code: impl Into<String>,
        error: impl Into<String>,
    ) -> Result<(), UploadQueueError> {
        let job = self.job_for_claim(job_id, claim_token)?;
        job.status = UploadStatus::Failed;
        job.queue_state = UploadQueueState::RetryScheduled;
        job.next_attempt_at = Some(next_attempt_at);
        job.safe_failure_code = Some(safe_failure_code.into());
        job.last_error = Some(error.into());
        job.claim_token = None;
        job.lease_expires_at = None;
        job.updated_at = Utc::now();
        Ok(())
    }

    pub fn mark_uncertain(
        &mut self,
        job_id: Uuid,
        claim_token: Uuid,
        safe_failure_code: impl Into<String>,
    ) -> Result<(), UploadQueueError> {
        let job = self.job_for_claim(job_id, claim_token)?;
        job.status = UploadStatus::Failed;
        job.queue_state = UploadQueueState::Uncertain;
        job.uncertain_outcome = true;
        job.safe_failure_code = Some(safe_failure_code.into());
        job.claim_token = None;
        job.lease_expires_at = None;
        job.updated_at = Utc::now();
        Ok(())
    }

    pub fn mark_completed(
        &mut self,
        job_id: Uuid,
        message: Option<String>,
    ) -> Result<UploadResult, UploadQueueError> {
        let Some(job) = self.jobs.iter_mut().find(|job| job.upload_job_id == job_id) else {
            return Err(UploadQueueError::JobNotFound(job_id));
        };
        job.status = UploadStatus::Completed;
        job.queue_state = UploadQueueState::Succeeded;
        job.updated_at = Utc::now();
        job.claim_token = None;
        job.lease_expires_at = None;
        job.next_attempt_at = None;
        job.uncertain_outcome = false;
        for item in &mut job.items {
            item.status = UploadStatus::Completed;
        }
        Ok(UploadResult {
            upload_job_id: job.upload_job_id,
            target_id: job.target_id.clone(),
            status: job.status.clone(),
            accepted_count: job.items.len(),
            failed_count: 0,
            message,
        })
    }

    pub fn mark_failed(
        &mut self,
        job_id: Uuid,
        error: impl Into<String>,
    ) -> Result<UploadResult, UploadQueueError> {
        let error = error.into();
        let Some(job) = self.jobs.iter_mut().find(|job| job.upload_job_id == job_id) else {
            return Err(UploadQueueError::JobNotFound(job_id));
        };
        job.status = UploadStatus::Failed;
        job.queue_state = UploadQueueState::DeadLetter;
        job.updated_at = Utc::now();
        job.last_error = Some(error.clone());
        job.safe_failure_code = Some("upload_failed".to_owned());
        job.claim_token = None;
        job.lease_expires_at = None;
        for item in &mut job.items {
            item.status = UploadStatus::Failed;
            item.error = Some(error.clone());
        }
        Ok(UploadResult {
            upload_job_id: job.upload_job_id,
            target_id: job.target_id.clone(),
            status: job.status.clone(),
            accepted_count: 0,
            failed_count: job.items.len(),
            message: Some(error),
        })
    }

    fn job_for_claim(
        &mut self,
        job_id: Uuid,
        claim_token: Uuid,
    ) -> Result<&mut UploadJob, UploadQueueError> {
        let job = self
            .jobs
            .iter_mut()
            .find(|job| job.upload_job_id == job_id)
            .ok_or(UploadQueueError::JobNotFound(job_id))?;
        match job.claim_token {
            Some(token) if token == claim_token => Ok(job),
            Some(_) => Err(UploadQueueError::ClaimTokenMismatch(job_id)),
            None => Err(UploadQueueError::AlreadyClaimed(job_id)),
        }
    }
}

pub fn upload_idempotency_key(provider_id: &str, logbook_id: Uuid, qso_ids: &[Uuid]) -> String {
    let mut qso_ids = qso_ids.to_vec();
    qso_ids.sort();
    format!(
        "upload.adif:{provider_id}:{logbook_id}:{}",
        qso_ids
            .iter()
            .map(Uuid::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}

pub fn select_qsos_for_upload(
    projection: &QsoCurrentStateProjection,
    qso_ids: Option<&[Uuid]>,
) -> Vec<QsoRecord> {
    let mut qsos = projection
        .list(false)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    if let Some(qso_ids) = qso_ids {
        qsos.retain(|qso| qso_ids.contains(&qso.qso_id));
    }
    qsos
}

pub fn adif_for_upload_job(projection: &QsoCurrentStateProjection, qso_ids: &[Uuid]) -> String {
    let mut filtered = QsoCurrentStateProjection::new();
    for qso in select_qsos_for_upload(projection, Some(qso_ids)) {
        filtered.upsert_record(qso);
    }
    export_adif(&filtered, false)
}

pub fn build_log_upload_request(
    provider_id: impl Into<String>,
    logbook_id: Uuid,
    projection: &QsoCurrentStateProjection,
    qso_ids: &[Uuid],
) -> LogUploadRequest {
    LogUploadRequest {
        job_id: Uuid::new_v4(),
        logbook_id,
        provider_id: Some(provider_id.into()),
        adif_payload: adif_for_upload_job(projection, qso_ids),
        incremental: true,
    }
}

pub async fn append_upload_status_event<S: LogbookEventStore>(
    store: &S,
    logbook_id: Uuid,
    event_type: &str,
    job: &UploadJob,
    source_device_id: Uuid,
    source_plugin_id: Option<String>,
    details: Value,
) -> Result<CoreEventEnvelope, UploadQueueError> {
    let event_type = match event_type {
        OFFICIAL_LOG_UPLOAD_QUEUED | OFFICIAL_LOG_UPLOAD_COMPLETED | OFFICIAL_LOG_UPLOAD_FAILED => {
            event_type
        }
        _ => OFFICIAL_LOG_UPLOAD_QUEUED,
    };
    Ok(store
        .append_event(NewLogbookEvent {
            event_type: event_type.to_owned(),
            logbook_id,
            entity_id: Some(job.upload_job_id),
            author_operator_id: None,
            station_callsign: "SYSTEM".to_owned(),
            operator_callsign: None,
            source_device_id,
            author_device_id: source_device_id,
            correlation_id: Uuid::new_v4(),
            source_plugin_id,
            schema_version: 1,
            payload: json!({
                "upload_job_id": job.upload_job_id,
                "target_id": job.target_id,
                "qso_ids": job.qso_ids,
                "status": job.status,
                "details": details,
            }),
        })
        .await?)
}

#[cfg(test)]
mod tests {
    use crate::InMemoryLogbookEventStore;

    use super::*;

    fn qso(id: Uuid, deleted: bool) -> QsoRecord {
        QsoRecord {
            qso_id: id,
            payload: json!({
                "qso_id": id,
                "contacted_callsign": "K1ABC",
                "station_callsign": "KE8YGW",
                "mode": "SSB",
                "started_at": "2026-07-06T12:00:00Z"
            }),
            note_history: Vec::new(),
            deleted,
            last_event_hash: "hash".to_owned(),
        }
    }

    #[test]
    fn creates_upload_job_and_selects_qsos() {
        let mut queue = UploadQueue::new(vec![UploadTarget {
            target_id: "lotw".to_owned(),
            display_name: "LoTW".to_owned(),
            provider_id: "lotw.stub".to_owned(),
            enabled: true,
            required_config_keys: vec!["lotw.certificate_path".to_owned()],
        }]);
        let qso_id = Uuid::new_v4();
        let job = queue
            .create_job("lotw", Uuid::new_v4(), vec![qso_id])
            .unwrap();
        assert_eq!(job.items.len(), 1);
        assert_eq!(job.queue_state, UploadQueueState::Pending);
        assert_eq!(job.operation_type, "upload.adif");
        assert_eq!(job.provider_id, "lotw.stub");
        assert!(!job.idempotency_key.is_empty());
    }

    #[test]
    fn queue_claims_are_exclusive_and_recover_after_lease_expiration() {
        let mut queue = UploadQueue::new(vec![UploadTarget {
            target_id: "lotw".to_owned(),
            display_name: "LoTW".to_owned(),
            provider_id: "lotw".to_owned(),
            enabled: true,
            required_config_keys: vec![],
        }]);
        let now = Utc::now();
        let job = queue
            .create_job("lotw", Uuid::new_v4(), vec![Uuid::new_v4()])
            .unwrap();
        let (claimed_id, token) = queue.claim_next(now, 30).unwrap();
        assert_eq!(claimed_id, job.upload_job_id);
        assert!(queue.claim_next(now, 30).is_none());
        assert!(matches!(
            queue.schedule_retry(
                claimed_id,
                Uuid::new_v4(),
                now + chrono::Duration::minutes(1),
                "temporary_failure",
                "temporary"
            ),
            Err(UploadQueueError::ClaimTokenMismatch(_))
        ));
        queue
            .schedule_retry(
                claimed_id,
                token,
                now + chrono::Duration::minutes(1),
                "temporary_failure",
                "temporary",
            )
            .unwrap();
        assert_eq!(queue.jobs[0].queue_state, UploadQueueState::RetryScheduled);
        assert!(queue.claim_next(now, 30).is_none());
        let (claimed_again, _) = queue
            .claim_next(now + chrono::Duration::minutes(2), 30)
            .unwrap();
        assert_eq!(claimed_again, claimed_id);
        assert_eq!(
            queue.recover_expired_leases(now + chrono::Duration::minutes(3)),
            1
        );
        assert_eq!(queue.jobs[0].queue_state, UploadQueueState::Uncertain);
        assert!(queue.jobs[0].uncertain_outcome);
    }

    #[test]
    fn idempotency_key_is_stable_for_duplicate_delivery() {
        let logbook_id = Uuid::new_v4();
        let left = Uuid::new_v4();
        let right = Uuid::new_v4();
        let one = upload_idempotency_key("clublog", logbook_id, &[left, right]);
        let two = upload_idempotency_key("clublog", logbook_id, &[right, left]);
        assert_eq!(one, two);

        let mut queue = UploadQueue::new(vec![UploadTarget {
            target_id: "clublog".to_owned(),
            display_name: "Club Log".to_owned(),
            provider_id: "clublog".to_owned(),
            enabled: true,
            required_config_keys: vec![],
        }]);
        let job = queue
            .create_job("clublog", logbook_id, vec![left, right])
            .unwrap();
        assert_eq!(
            queue
                .find_duplicate(&job.idempotency_key)
                .unwrap()
                .upload_job_id,
            job.upload_job_id
        );
    }

    #[test]
    fn queue_limit_reports_overflow() {
        let mut queue = UploadQueue::new(vec![UploadTarget {
            target_id: "eqsl".to_owned(),
            display_name: "eQSL".to_owned(),
            provider_id: "eqsl".to_owned(),
            enabled: true,
            required_config_keys: vec![],
        }]);
        queue.queue_limit = 1;
        queue
            .create_job("eqsl", Uuid::new_v4(), vec![Uuid::new_v4()])
            .unwrap();
        assert!(matches!(
            queue.create_job("eqsl", Uuid::new_v4(), vec![Uuid::new_v4()]),
            Err(UploadQueueError::QueueFull)
        ));
    }

    #[test]
    fn deleted_qsos_are_excluded_from_upload_selection() {
        let visible = Uuid::new_v4();
        let deleted = Uuid::new_v4();
        let mut projection = QsoCurrentStateProjection::new();
        projection.upsert_record(qso(visible, false));
        projection.upsert_record(qso(deleted, true));
        assert_eq!(select_qsos_for_upload(&projection, None).len(), 1);
    }

    #[test]
    fn upload_job_generates_adif() {
        let qso_id = Uuid::new_v4();
        let mut projection = QsoCurrentStateProjection::new();
        projection.upsert_record(qso(qso_id, false));
        let adif = adif_for_upload_job(&projection, &[qso_id]);
        assert!(adif.contains("<CALL:5>K1ABC"));
    }

    #[tokio::test]
    async fn official_upload_event_appends() {
        let store = InMemoryLogbookEventStore::default();
        let logbook_id = Uuid::new_v4();
        let mut queue = UploadQueue::new(vec![UploadTarget {
            target_id: "qrz".to_owned(),
            display_name: "QRZ Logbook".to_owned(),
            provider_id: "qrz-logbook.stub".to_owned(),
            enabled: true,
            required_config_keys: Vec::new(),
        }]);
        let job = queue
            .create_job("qrz", logbook_id, vec![Uuid::new_v4()])
            .unwrap();
        let event = append_upload_status_event(
            &store,
            logbook_id,
            OFFICIAL_LOG_UPLOAD_QUEUED,
            &job,
            Uuid::new_v4(),
            Some("core.upload-queue".to_owned()),
            json!({}),
        )
        .await
        .unwrap();
        assert_eq!(event.event_type, OFFICIAL_LOG_UPLOAD_QUEUED);
        assert!(store.verify_chain(logbook_id).await.is_ok());
    }
}
