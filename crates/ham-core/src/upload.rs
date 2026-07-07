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

#[derive(Debug, Error)]
pub enum UploadQueueError {
    #[error("upload target {0} was not found")]
    TargetNotFound(String),
    #[error("upload provider {0} is missing required configuration: {1:?}")]
    MissingProviderConfig(String, Vec<String>),
    #[error("upload job {0} was not found")]
    JobNotFound(Uuid),
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
    pub logbook_id: Uuid,
    pub qso_ids: Vec<Uuid>,
    pub items: Vec<UploadJobItem>,
    pub status: UploadStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_error: Option<String>,
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
}

impl UploadQueue {
    pub fn new(targets: Vec<UploadTarget>) -> Self {
        Self {
            targets,
            jobs: VecDeque::new(),
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
        let now = Utc::now();
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
            logbook_id,
            qso_ids,
            items,
            status: UploadStatus::Queued,
            created_at: now,
            updated_at: now,
            last_error: None,
        };
        self.jobs.push_back(job.clone());
        Ok(job)
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
        job.updated_at = Utc::now();
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
        job.updated_at = Utc::now();
        job.last_error = Some(error.clone());
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
