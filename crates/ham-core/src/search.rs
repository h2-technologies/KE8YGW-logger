use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::QsoRecord;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SearchError {
    #[error("invalid date range {0}")]
    InvalidDateRange(String),
    #[error("saved search {0} was not found")]
    SavedSearchNotFound(Uuid),
}

#[derive(Debug, Error)]
pub enum SavedSearchStoreError {
    #[error("saved search storage I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("saved search storage JSON failed: {0}")]
    Serde(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateRange {
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchFilter {
    pub field: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQuery {
    pub raw: String,
    pub filters: Vec<SearchFilter>,
    pub text_terms: Vec<String>,
    pub date_range: Option<DateRange>,
    pub include_deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResult {
    pub qso_id: Uuid,
    pub score: u16,
    pub matched_fields: Vec<String>,
    pub payload: Value,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSearch {
    pub saved_search_id: Uuid,
    pub name: String,
    pub query: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSearchBook {
    pub searches: Vec<SavedSearch>,
}

impl SavedSearchBook {
    pub fn save_search(
        &mut self,
        name: impl Into<String>,
        query: impl Into<String>,
    ) -> SavedSearch {
        let search = SavedSearch {
            saved_search_id: Uuid::new_v4(),
            name: name.into(),
            query: query.into(),
            created_at: Utc::now(),
        };
        self.searches.push(search.clone());
        search
    }
}

#[derive(Debug, Clone)]
pub struct JsonSavedSearchStore {
    path: PathBuf,
}

impl JsonSavedSearchStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self) -> Result<SavedSearchBook, SavedSearchStoreError> {
        if !self.path.exists() {
            return Ok(SavedSearchBook::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(&self.path)?)?)
    }

    pub fn save(&self, book: &SavedSearchBook) -> Result<(), SavedSearchStoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, serde_json::to_vec_pretty(book)?)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn parse_search_query(input: &str) -> Result<SearchQuery, SearchError> {
    let mut filters = Vec::new();
    let mut text_terms = Vec::new();
    let mut date_range = None;
    let mut include_deleted = false;

    for token in input.split_whitespace() {
        if let Some((field, value)) = token.split_once(':') {
            let field = field.trim().to_ascii_lowercase();
            let value = value.trim().to_owned();
            if field == "date" {
                date_range = Some(parse_date_range(&value)?);
            } else if field == "deleted" {
                include_deleted = matches!(
                    value.to_ascii_lowercase().as_str(),
                    "true" | "yes" | "1" | "include"
                );
                filters.push(SearchFilter { field, value });
            } else {
                filters.push(SearchFilter { field, value });
            }
        } else if !token.trim().is_empty() {
            text_terms.push(token.trim().to_ascii_lowercase());
        }
    }

    Ok(SearchQuery {
        raw: input.to_owned(),
        filters,
        text_terms,
        date_range,
        include_deleted,
    })
}

pub fn search_qsos(qsos: &[QsoRecord], query: &SearchQuery) -> Vec<SearchResult> {
    let mut results = Vec::new();
    for qso in qsos {
        if qso.deleted && !query.include_deleted {
            continue;
        }
        if !matches_date(qso, query.date_range.as_ref()) {
            continue;
        }
        let mut matched_fields = Vec::new();
        if !matches_filters(qso, &query.filters, &mut matched_fields) {
            continue;
        }
        if !matches_text(qso, &query.text_terms, &mut matched_fields) {
            continue;
        }
        results.push(SearchResult {
            qso_id: qso.qso_id,
            score: matched_fields.len() as u16,
            matched_fields,
            payload: qso.payload.clone(),
            deleted: qso.deleted,
        });
    }
    results
}

fn parse_date_range(input: &str) -> Result<DateRange, SearchError> {
    if let Some((start, end)) = input.split_once("..") {
        return Ok(DateRange {
            start: parse_date_endpoint(start, false)?,
            end: parse_date_endpoint(end, true)?,
        });
    }
    let start = parse_date_endpoint(input, false)?;
    let end = parse_date_endpoint(input, true)?;
    Ok(DateRange { start, end })
}

fn parse_date_endpoint(
    input: &str,
    end_of_day: bool,
) -> Result<Option<DateTime<Utc>>, SearchError> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(None);
    }
    let date = NaiveDate::parse_from_str(input, "%Y-%m-%d")
        .map_err(|_| SearchError::InvalidDateRange(input.to_owned()))?;
    let naive = if end_of_day {
        date.and_hms_opt(23, 59, 59)
    } else {
        date.and_hms_opt(0, 0, 0)
    };
    Ok(naive.map(|value| Utc.from_utc_datetime(&value)))
}

fn matches_date(qso: &QsoRecord, range: Option<&DateRange>) -> bool {
    let Some(range) = range else {
        return true;
    };
    let Some(started_at) = qso
        .payload
        .get("started_at")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
    else {
        return false;
    };
    if let Some(start) = range.start {
        if started_at < start {
            return false;
        }
    }
    if let Some(end) = range.end {
        if started_at > end {
            return false;
        }
    }
    true
}

fn matches_filters(
    qso: &QsoRecord,
    filters: &[SearchFilter],
    matched_fields: &mut Vec<String>,
) -> bool {
    for filter in filters {
        if filter.field == "date" {
            continue;
        }
        if filter.field == "deleted" {
            let expected = matches!(
                filter.value.to_ascii_lowercase().as_str(),
                "true" | "yes" | "1" | "include"
            );
            if qso.deleted != expected {
                return false;
            }
            matched_fields.push("deleted".to_owned());
            continue;
        }
        let Some(actual) = field_value(qso, &filter.field) else {
            return false;
        };
        if !actual
            .to_ascii_lowercase()
            .contains(&filter.value.to_ascii_lowercase())
        {
            return false;
        }
        matched_fields.push(filter.field.clone());
    }
    true
}

fn matches_text(qso: &QsoRecord, terms: &[String], matched_fields: &mut Vec<String>) -> bool {
    if terms.is_empty() {
        return true;
    }
    let haystack = ["contacted_callsign", "notes", "name", "qth", "tags"]
        .into_iter()
        .filter_map(|field| field_value(qso, field))
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    for term in terms {
        if !haystack.contains(term) {
            return false;
        }
    }
    matched_fields.push("text".to_owned());
    true
}

fn field_value(qso: &QsoRecord, field: &str) -> Option<String> {
    let payload_field = match field {
        "callsign" => "contacted_callsign",
        "park" => "park_id",
        "summit" => "summit_id",
        "operator" => "operator_callsign",
        "station" => "station_callsign",
        "profile" => "station_profile_id",
        "tag" => "tags",
        other => other,
    };
    let value = qso.payload.get(payload_field)?;
    if let Some(string) = value.as_str() {
        return Some(string.to_owned());
    }
    if value.is_array() {
        return Some(
            value
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .filter_map(|value| value.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    Some(value.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn qso(payload: Value, deleted: bool) -> QsoRecord {
        QsoRecord {
            qso_id: Uuid::new_v4(),
            payload,
            note_history: Vec::new(),
            deleted,
            last_event_hash: "hash".to_owned(),
        }
    }

    #[test]
    fn parses_field_filters_and_plain_text() {
        let query = parse_search_query("callsign:K1ABC band:20m portable").unwrap();
        assert_eq!(query.filters.len(), 2);
        assert_eq!(query.text_terms, vec!["portable"]);
    }

    #[test]
    fn parses_date_ranges() {
        let query = parse_search_query("date:2026-07-01..2026-07-06").unwrap();
        assert!(query.date_range.unwrap().start.is_some());
    }

    #[test]
    fn combines_filters_and_plain_text() {
        let query = parse_search_query("band:20m mode:FT8 tag:portable Japan").unwrap();
        let records = vec![
            qso(
                json!({"contacted_callsign": "JA1ABC", "band": "20m", "mode": "FT8", "tags": ["portable"], "qth": "Japan"}),
                false,
            ),
            qso(
                json!({"contacted_callsign": "K1ABC", "band": "40m", "mode": "SSB"}),
                false,
            ),
        ];
        assert_eq!(search_qsos(&records, &query).len(), 1);
    }

    #[test]
    fn deleted_qsos_are_hidden_unless_requested() {
        let records = vec![qso(json!({"contacted_callsign": "K1ABC"}), true)];
        assert!(search_qsos(&records, &parse_search_query("K1ABC").unwrap()).is_empty());
        assert_eq!(
            search_qsos(&records, &parse_search_query("deleted:true K1ABC").unwrap()).len(),
            1
        );
    }

    #[test]
    fn saved_searches_persist() {
        let path = std::env::temp_dir().join(format!("saved-search-{}.json", Uuid::new_v4()));
        let store = JsonSavedSearchStore::new(&path);
        let mut book = SavedSearchBook::default();
        book.save_search("Portable", "tag:portable");
        store.save(&book).unwrap();
        assert_eq!(store.load().unwrap().searches.len(), 1);
        let _ = fs::remove_file(path);
    }
}
