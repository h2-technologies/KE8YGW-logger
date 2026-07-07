use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::QsoRecord;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AwardRequirement {
    UniqueEntity,
    UniqueState,
    UniquePark,
    UniqueSummit,
    UniqueGrid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AwardRule {
    pub requirement: AwardRequirement,
    pub field: String,
    pub confirmed_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AwardDefinition {
    pub award_id: String,
    pub name: String,
    pub plugin_id: String,
    pub description: String,
    pub rules: Vec<AwardRule>,
    pub placeholder: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AwardCredit {
    pub credit_key: String,
    pub display_name: String,
    pub qso_ids: Vec<Uuid>,
    pub confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AwardProgress {
    pub award_id: String,
    pub name: String,
    pub credit_count: usize,
    pub confirmed_credit_count: usize,
    pub credits: Vec<AwardCredit>,
    pub missing_placeholder: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AwardPlugin {
    pub plugin_id: String,
    pub definitions: Vec<AwardDefinition>,
}

#[derive(Debug, Default, Clone)]
pub struct AwardEngine {
    definitions: Vec<AwardDefinition>,
}

impl AwardEngine {
    pub fn new(definitions: Vec<AwardDefinition>) -> Self {
        Self { definitions }
    }

    pub fn default_mvp() -> Self {
        Self::new(default_award_definitions())
    }

    pub fn definitions(&self) -> &[AwardDefinition] {
        &self.definitions
    }

    pub fn rebuild_from_qsos(&self, qsos: &[QsoRecord]) -> Vec<AwardProgress> {
        self.definitions
            .iter()
            .map(|definition| compute_award_progress(definition, qsos))
            .collect()
    }
}

pub fn default_award_definitions() -> Vec<AwardDefinition> {
    vec![
        definition(
            "dxcc.basic",
            "DXCC",
            "Unique DXCC/entity count from accepted QSO projection fields.",
            "dxcc",
            AwardRequirement::UniqueEntity,
            false,
        ),
        definition(
            "was.basic",
            "WAS",
            "Unique US state count when state data is present.",
            "state",
            AwardRequirement::UniqueState,
            false,
        ),
        placeholder_definition(
            "pota.unique_parks",
            "POTA",
            "Unique POTA parks worked or activated. Confirmation and activator/hunter modes are future work.",
            "park_id",
            AwardRequirement::UniquePark,
        ),
        placeholder_definition(
            "sota.unique_summits",
            "SOTA",
            "Unique SOTA summits worked or activated. Confirmation and activator/hunter modes are future work.",
            "summit_id",
            AwardRequirement::UniqueSummit,
        ),
        placeholder_definition(
            "grid.count",
            "Grids",
            "Unique Maidenhead grid count from projected QSO fields.",
            "grid",
            AwardRequirement::UniqueGrid,
        ),
    ]
}

pub fn compute_award_progress(definition: &AwardDefinition, qsos: &[QsoRecord]) -> AwardProgress {
    let Some(rule) = definition.rules.first() else {
        return AwardProgress {
            award_id: definition.award_id.clone(),
            name: definition.name.clone(),
            credit_count: 0,
            confirmed_credit_count: 0,
            credits: Vec::new(),
            missing_placeholder: Vec::new(),
        };
    };
    let mut credits: BTreeMap<String, AwardCredit> = BTreeMap::new();
    for qso in qsos.iter().filter(|qso| !qso.deleted) {
        let Some(key) = qso
            .payload
            .get(&rule.field)
            .and_then(scalar_key)
            .map(|value| value.to_ascii_uppercase())
        else {
            continue;
        };
        let confirmed = qso
            .payload
            .get("confirmed")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if rule.confirmed_only && !confirmed {
            continue;
        }
        let display_name = display_value(qso, &rule.field, &key);
        let entry = credits.entry(key.clone()).or_insert_with(|| AwardCredit {
            credit_key: key,
            display_name,
            qso_ids: Vec::new(),
            confirmed,
        });
        entry.qso_ids.push(qso.qso_id);
        entry.confirmed |= confirmed;
    }

    let credits = credits.into_values().collect::<Vec<_>>();
    AwardProgress {
        award_id: definition.award_id.clone(),
        name: definition.name.clone(),
        credit_count: credits.len(),
        confirmed_credit_count: credits.iter().filter(|credit| credit.confirmed).count(),
        credits,
        missing_placeholder: missing_placeholders(definition),
    }
}

fn definition(
    award_id: &str,
    name: &str,
    description: &str,
    field: &str,
    requirement: AwardRequirement,
    confirmed_only: bool,
) -> AwardDefinition {
    AwardDefinition {
        award_id: award_id.to_owned(),
        name: name.to_owned(),
        plugin_id: "core.awards".to_owned(),
        description: description.to_owned(),
        rules: vec![AwardRule {
            requirement,
            field: field.to_owned(),
            confirmed_only,
        }],
        placeholder: false,
    }
}

fn placeholder_definition(
    award_id: &str,
    name: &str,
    description: &str,
    field: &str,
    requirement: AwardRequirement,
) -> AwardDefinition {
    let mut definition = definition(award_id, name, description, field, requirement, false);
    definition.placeholder = true;
    definition
}

fn display_value(qso: &QsoRecord, field: &str, fallback: &str) -> String {
    match field {
        "dxcc" => qso
            .payload
            .get("entity")
            .and_then(|value| value.as_str())
            .unwrap_or(fallback)
            .to_owned(),
        _ => fallback.to_owned(),
    }
}

fn scalar_key(value: &serde_json::Value) -> Option<String> {
    if let Some(value) = value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(value.to_owned());
    }
    if value.is_number() || value.is_boolean() {
        return Some(value.to_string());
    }
    None
}

fn missing_placeholders(definition: &AwardDefinition) -> Vec<String> {
    if !definition.placeholder {
        return Vec::new();
    }
    BTreeSet::from([
        "confirmation integration".to_owned(),
        "award-specific rule database".to_owned(),
    ])
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn qso(payload: serde_json::Value, deleted: bool) -> QsoRecord {
        QsoRecord {
            qso_id: Uuid::new_v4(),
            payload,
            note_history: Vec::new(),
            deleted,
            last_event_hash: "hash".to_owned(),
        }
    }

    #[test]
    fn dxcc_counts_unique_entities_and_ignores_duplicates() {
        let engine = AwardEngine::default_mvp();
        let progress = engine.rebuild_from_qsos(&[
            qso(json!({"dxcc": 291, "entity": "United States"}), false),
            qso(json!({"dxcc": 291, "entity": "United States"}), false),
            qso(json!({"dxcc": 339, "entity": "Japan"}), false),
        ]);
        let dxcc = progress
            .iter()
            .find(|progress| progress.award_id == "dxcc.basic")
            .unwrap();
        assert_eq!(dxcc.credit_count, 2);
    }

    #[test]
    fn deleted_qso_does_not_count_and_restored_qso_counts_again() {
        let engine = AwardEngine::default_mvp();
        let deleted = engine.rebuild_from_qsos(&[qso(json!({"dxcc": 291}), true)]);
        assert_eq!(deleted[0].credit_count, 0);

        let restored = engine.rebuild_from_qsos(&[qso(json!({"dxcc": 291}), false)]);
        assert_eq!(restored[0].credit_count, 1);
    }

    #[test]
    fn was_counts_unique_states_when_available() {
        let engine = AwardEngine::default_mvp();
        let progress = engine.rebuild_from_qsos(&[
            qso(json!({"state": "OH"}), false),
            qso(json!({"state": "OH"}), false),
            qso(json!({"state": "MI"}), false),
        ]);
        let was = progress
            .iter()
            .find(|progress| progress.award_id == "was.basic")
            .unwrap();
        assert_eq!(was.credit_count, 2);
    }

    #[test]
    fn placeholder_awards_load() {
        let definitions = default_award_definitions();
        assert!(definitions
            .iter()
            .any(|award| award.award_id == "pota.unique_parks"));
        assert!(definitions
            .iter()
            .any(|award| award.award_id == "sota.unique_summits"));
        assert!(definitions
            .iter()
            .any(|award| award.award_id == "grid.count"));
    }
}
