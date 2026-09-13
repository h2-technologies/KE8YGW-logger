//! Versioned contest rule and exchange schema.
//!
//! Contest definitions are data, not code. A definition pack is a signed,
//! versioned JSON document that the application loads at runtime, so a new or
//! corrected contest definition can be delivered without shipping a new
//! application build. Everything in this module is deliberately declarative:
//! it describes what a contest requires, and rejects anything it does not
//! fully understand instead of guessing.
//!
//! Two independent versions guard compatibility:
//!
//! * `schema_version` describes the shape of the pack itself. A pack written
//!   against a newer schema than this build supports is rejected whole.
//! * `rule_version` describes one contest's rules. A catalog keeps the highest
//!   rule version it trusts for each contest and records where it came from.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Pack schema version understood by this build.
pub const CONTEST_RULE_SCHEMA_VERSION: u32 = 1;

/// Discriminator every contest definition pack must carry.
pub const CONTEST_DEFINITION_PACK_KIND: &str = "ke8ygw.contest.definition-pack";

/// Built-in definitions compiled into the application.
///
/// These are loaded through exactly the same path as an operator-installed
/// pack so the bundled data cannot drift from the supported schema.
const BUILTIN_PACK_JSON: &str = include_str!("../assets/contest-definitions-v1.json");

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContestSchemaError {
    #[error("contest definition pack is not valid JSON: {0}")]
    InvalidJson(String),
    #[error("unsupported contest definition pack kind `{found}`, expected `{expected}`")]
    UnsupportedPackKind { found: String, expected: String },
    #[error("unsupported contest rule schema version {found}, this build supports {supported}")]
    UnsupportedSchemaVersion { found: u32, supported: u32 },
    #[error("contest definition pack repeats contest id `{0}`")]
    DuplicateContestId(String),
    #[error("contest `{contest_id}` is not usable: {reason}")]
    InvalidDefinition { contest_id: String, reason: String },
    #[error("contest definition pack `{0}` has no definitions")]
    EmptyPack(String),
    #[error("unknown contest id `{0}`")]
    UnknownContest(String),
    #[error("contest definition pack is unsigned and unsigned packs are not trusted")]
    SignatureRequired,
    #[error("contest definition pack was signed with unknown key id `{0}`")]
    UnknownSigningKey(String),
    #[error("contest definition pack signature does not match its content")]
    SignatureInvalid,
    #[error("contest definition pack digest does not match its content")]
    DigestMismatch,
    #[error("exchange field `{key}` is required and was not provided")]
    MissingExchangeField { key: String },
    #[error("exchange field `{key}` is not part of this contest exchange")]
    UnknownExchangeField { key: String },
    #[error("exchange field `{key}` is not valid: {reason}")]
    InvalidExchangeField { key: String, reason: String },
}

/// Which side of the exchange a field belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExchangeDirection {
    Sent,
    Received,
}

/// The value shape of one exchange field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExchangeFieldKind {
    /// A contest serial number allocated by the serial policy.
    Serial,
    /// A signal report such as `59` or `599`.
    Rst,
    /// A callsign, normalized to upper case.
    Callsign,
    /// A Maidenhead grid square, normalized to standard casing.
    Grid,
    /// Free text, bounded so exports stay within official column widths.
    Text { max_len: usize },
    /// A whole number inside an inclusive range.
    Integer { min: i64, max: i64 },
    /// One of a fixed set of values, compared case-insensitively.
    Choice { options: Vec<String> },
}

/// One field of a contest exchange.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExchangeField {
    pub key: String,
    pub label: String,
    pub kind: ExchangeFieldKind,
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_true() -> bool {
    true
}

/// The sent and received halves of a contest exchange.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContestExchange {
    pub sent: Vec<ExchangeField>,
    pub received: Vec<ExchangeField>,
}

impl ContestExchange {
    pub fn fields(&self, direction: ExchangeDirection) -> &[ExchangeField] {
        match direction {
            ExchangeDirection::Sent => &self.sent,
            ExchangeDirection::Received => &self.received,
        }
    }

    pub fn field(&self, direction: ExchangeDirection, key: &str) -> Option<&ExchangeField> {
        self.fields(direction).iter().find(|field| field.key == key)
    }
}

/// The entry-category dimensions a contest recognizes.
///
/// Each list is the set of accepted values for that dimension. An empty list
/// means the contest does not use the dimension.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContestCategories {
    pub operator: Vec<String>,
    pub transmitter: Vec<String>,
    pub power: Vec<String>,
    pub band: Vec<String>,
    pub mode: Vec<String>,
    pub assisted: Vec<String>,
    pub overlay: Vec<String>,
}

/// The scope in which a worked station counts as a duplicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateScope {
    /// One contact per callsign for the whole contest.
    PerContest,
    /// One contact per callsign per band, regardless of mode.
    PerBand,
    /// One contact per callsign per mode, regardless of band.
    PerMode,
    /// One contact per callsign per band and mode.
    PerBandMode,
}

/// How duplicates are detected for one contest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicateRule {
    pub scope: DuplicateScope,
    /// When set, a repeat contact is allowed again after this many minutes.
    #[serde(default)]
    pub repeat_after_minutes: Option<u32>,
}

impl DuplicateRule {
    /// The comparison key for one contact under this rule.
    ///
    /// Callers compare keys; they never re-implement the scope logic.
    pub fn duplicate_key(&self, callsign: &str, band: &str, mode: &str) -> String {
        let callsign = callsign.trim().to_ascii_uppercase();
        let band = band.trim().to_ascii_uppercase();
        let mode = mode.trim().to_ascii_uppercase();
        match self.scope {
            DuplicateScope::PerContest => callsign,
            DuplicateScope::PerBand => format!("{callsign}|{band}"),
            DuplicateScope::PerMode => format!("{callsign}|{mode}"),
            DuplicateScope::PerBandMode => format!("{callsign}|{band}|{mode}"),
        }
    }
}

/// How serial numbers are allocated during a contest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SerialPolicy {
    /// The contest has no serial number.
    None,
    /// One increasing sequence for the whole contest.
    PerContest,
    /// A separate sequence per band.
    PerBand,
    /// A separate sequence per band and mode.
    PerBandMode,
}

impl SerialPolicy {
    /// The sequence key a serial number is drawn from, if any.
    pub fn sequence_key(self, band: &str, mode: &str) -> Option<String> {
        let band = band.trim().to_ascii_uppercase();
        let mode = mode.trim().to_ascii_uppercase();
        match self {
            Self::None => None,
            Self::PerContest => Some("contest".to_owned()),
            Self::PerBand => Some(format!("band|{band}")),
            Self::PerBandMode => Some(format!("band|{band}|mode|{mode}")),
        }
    }
}

/// Where a multiplier value is read from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MultiplierSource {
    /// The value of a received exchange field.
    ReceivedExchangeField { field_key: String },
    /// The DXCC entity of the worked station.
    DxccEntity,
    /// The worked station's grid square, truncated to `precision` characters.
    GridSquare { precision: usize },
}

/// Whether a multiplier counts once overall or once per band or mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultiplierScope {
    PerContest,
    PerBand,
    PerMode,
    PerBandMode,
}

/// One multiplier a contest counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MultiplierRule {
    pub key: String,
    pub label: String,
    pub source: MultiplierSource,
    pub scope: MultiplierScope,
}

/// A points rule. Empty `bands` or `modes` match every band or mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointRule {
    #[serde(default)]
    pub bands: Vec<String>,
    #[serde(default)]
    pub modes: Vec<String>,
    pub points: i64,
}

impl PointRule {
    fn matches(&self, band: &str, mode: &str) -> bool {
        matches_any(&self.bands, band) && matches_any(&self.modes, mode)
    }
}

/// How a contest scores contacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContestScoring {
    pub default_points: i64,
    #[serde(default)]
    pub rules: Vec<PointRule>,
}

impl ContestScoring {
    /// Points for one contact. The first matching rule wins, so packs can
    /// order specific rules before general ones.
    pub fn points_for(&self, band: &str, mode: &str) -> i64 {
        self.rules
            .iter()
            .find(|rule| rule.matches(band, mode))
            .map_or(self.default_points, |rule| rule.points)
    }
}

/// A window during which contacts count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContestTimeWindow {
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

impl ContestTimeWindow {
    pub fn contains(&self, at: DateTime<Utc>) -> bool {
        at >= self.starts_at && at <= self.ends_at
    }
}

/// Identifying metadata an official export needs.
///
/// This module records the identity only; export formatting lives with the
/// export implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContestExportProfile {
    pub cabrillo_name: String,
    pub cabrillo_version: String,
}

/// One contest's complete v1 rule set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContestDefinition {
    pub contest_id: String,
    pub name: String,
    pub sponsor: String,
    /// Monotone version of this contest's rules. A catalog keeps the highest.
    pub rule_version: u32,
    /// Where the encoded rules were taken from, shown to the operator.
    pub rule_source: String,
    pub bands: Vec<String>,
    pub modes: Vec<String>,
    #[serde(default)]
    pub time_windows: Vec<ContestTimeWindow>,
    #[serde(default)]
    pub max_operating_minutes: Option<u32>,
    pub exchange: ContestExchange,
    #[serde(default)]
    pub categories: ContestCategories,
    pub duplicate_rule: DuplicateRule,
    pub serial_policy: SerialPolicy,
    #[serde(default)]
    pub multipliers: Vec<MultiplierRule>,
    pub scoring: ContestScoring,
    pub export: ContestExportProfile,
}

impl ContestDefinition {
    /// Whether the contest is running at `at`.
    ///
    /// A definition with no declared window is always available, which is how
    /// the generic templates are used outside a scheduled event.
    pub fn is_active_at(&self, at: DateTime<Utc>) -> bool {
        self.time_windows.is_empty() || self.time_windows.iter().any(|window| window.contains(at))
    }

    pub fn supports_band(&self, band: &str) -> bool {
        matches_any(&self.bands, band)
    }

    pub fn supports_mode(&self, mode: &str) -> bool {
        matches_any(&self.modes, mode)
    }

    /// Validate one side of an exchange against this contest.
    ///
    /// Returns the normalized values so callers store the same text the
    /// duplicate, scoring, and export paths will read.
    pub fn validate_exchange(
        &self,
        direction: ExchangeDirection,
        values: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, String>, ContestSchemaError> {
        let fields = self.exchange.fields(direction);
        for key in values.keys() {
            if !fields.iter().any(|field| &field.key == key) {
                return Err(ContestSchemaError::UnknownExchangeField { key: key.clone() });
            }
        }

        let mut normalized = BTreeMap::new();
        for field in fields {
            let raw = values.get(&field.key).map(|value| value.trim());
            match raw {
                None | Some("") => {
                    if field.required {
                        return Err(ContestSchemaError::MissingExchangeField {
                            key: field.key.clone(),
                        });
                    }
                }
                Some(value) => {
                    normalized.insert(field.key.clone(), normalize_exchange_value(field, value)?);
                }
            }
        }
        Ok(normalized)
    }

    fn validate_self(&self) -> Result<(), ContestSchemaError> {
        let invalid = |reason: &str| ContestSchemaError::InvalidDefinition {
            contest_id: self.contest_id.clone(),
            reason: reason.to_owned(),
        };

        if self.contest_id.trim().is_empty() {
            return Err(invalid("contest_id must not be blank"));
        }
        if self.name.trim().is_empty() {
            return Err(invalid("name must not be blank"));
        }
        if self.rule_version == 0 {
            return Err(invalid("rule_version must be greater than zero"));
        }
        if self.bands.is_empty() {
            return Err(invalid("at least one band is required"));
        }
        if self.modes.is_empty() {
            return Err(invalid("at least one mode is required"));
        }
        for window in &self.time_windows {
            if window.ends_at <= window.starts_at {
                return Err(invalid("every time window must end after it starts"));
            }
        }

        let mut seen = BTreeSet::new();
        for direction in [ExchangeDirection::Sent, ExchangeDirection::Received] {
            let fields = self.exchange.fields(direction);
            if fields.is_empty() {
                return Err(invalid(
                    "both exchange directions require at least one field",
                ));
            }
            for field in fields {
                if field.key.trim().is_empty() {
                    return Err(invalid("exchange field keys must not be blank"));
                }
                if !seen.insert((direction, field.key.clone())) {
                    return Err(invalid(&format!(
                        "exchange field `{}` is declared twice",
                        field.key
                    )));
                }
                match &field.kind {
                    ExchangeFieldKind::Text { max_len } if *max_len == 0 => {
                        return Err(invalid(&format!(
                            "text exchange field `{}` needs a non-zero max_len",
                            field.key
                        )));
                    }
                    ExchangeFieldKind::Integer { min, max } if min > max => {
                        return Err(invalid(&format!(
                            "integer exchange field `{}` has an inverted range",
                            field.key
                        )));
                    }
                    ExchangeFieldKind::Choice { options } if options.is_empty() => {
                        return Err(invalid(&format!(
                            "choice exchange field `{}` needs at least one option",
                            field.key
                        )));
                    }
                    _ => {}
                }
            }
        }

        let declares_serial = self
            .exchange
            .sent
            .iter()
            .chain(self.exchange.received.iter())
            .any(|field| field.kind == ExchangeFieldKind::Serial);
        match (self.serial_policy, declares_serial) {
            (SerialPolicy::None, true) => {
                return Err(invalid(
                    "the exchange declares a serial field but the serial policy is none",
                ));
            }
            (policy, false) if policy != SerialPolicy::None => {
                return Err(invalid(
                    "the serial policy allocates serials but no exchange field receives them",
                ));
            }
            _ => {}
        }

        let mut multiplier_keys = BTreeSet::new();
        for multiplier in &self.multipliers {
            if !multiplier_keys.insert(multiplier.key.clone()) {
                return Err(invalid(&format!(
                    "multiplier `{}` is declared twice",
                    multiplier.key
                )));
            }
            match &multiplier.source {
                MultiplierSource::ReceivedExchangeField { field_key } => {
                    if self
                        .exchange
                        .field(ExchangeDirection::Received, field_key)
                        .is_none()
                    {
                        return Err(invalid(&format!(
                            "multiplier `{}` reads received exchange field `{field_key}`, which is not declared",
                            multiplier.key
                        )));
                    }
                }
                MultiplierSource::GridSquare { precision } => {
                    if *precision == 0 || precision % 2 != 0 || *precision > 8 {
                        return Err(invalid(&format!(
                            "multiplier `{}` needs an even grid precision between 2 and 8",
                            multiplier.key
                        )));
                    }
                }
                MultiplierSource::DxccEntity => {}
            }
        }

        for rule in &self.scoring.rules {
            for band in &rule.bands {
                if !self.supports_band(band) {
                    return Err(invalid(&format!(
                        "a scoring rule references band `{band}`, which the contest does not use"
                    )));
                }
            }
            for mode in &rule.modes {
                if !self.supports_mode(mode) {
                    return Err(invalid(&format!(
                        "a scoring rule references mode `{mode}`, which the contest does not use"
                    )));
                }
            }
        }

        if self.export.cabrillo_name.trim().is_empty() {
            return Err(invalid("export.cabrillo_name must not be blank"));
        }

        Ok(())
    }
}

/// A versioned, self-describing set of contest definitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContestDefinitionPack {
    pub kind: String,
    pub schema_version: u32,
    pub pack_id: String,
    pub pack_version: u32,
    pub generated_at: DateTime<Utc>,
    pub definitions: Vec<ContestDefinition>,
}

impl ContestDefinitionPack {
    /// Canonical bytes used for digests and signatures.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("contest definition packs always serialize")
    }

    /// Lowercase hex SHA-256 of the canonical bytes.
    pub fn digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.canonical_bytes());
        hex_lower(&hasher.finalize())
    }

    pub fn definition(&self, contest_id: &str) -> Option<&ContestDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.contest_id == contest_id)
    }

    fn validate(&self) -> Result<(), ContestSchemaError> {
        if self.kind != CONTEST_DEFINITION_PACK_KIND {
            return Err(ContestSchemaError::UnsupportedPackKind {
                found: self.kind.clone(),
                expected: CONTEST_DEFINITION_PACK_KIND.to_owned(),
            });
        }
        if self.schema_version != CONTEST_RULE_SCHEMA_VERSION {
            return Err(ContestSchemaError::UnsupportedSchemaVersion {
                found: self.schema_version,
                supported: CONTEST_RULE_SCHEMA_VERSION,
            });
        }
        if self.definitions.is_empty() {
            return Err(ContestSchemaError::EmptyPack(self.pack_id.clone()));
        }

        let mut seen = BTreeSet::new();
        for definition in &self.definitions {
            if !seen.insert(definition.contest_id.clone()) {
                return Err(ContestSchemaError::DuplicateContestId(
                    definition.contest_id.clone(),
                ));
            }
            definition.validate_self()?;
        }
        Ok(())
    }
}

/// The signature algorithm a pack was signed with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackSignatureAlgorithm {
    HmacSha256,
}

/// A detached signature over a pack's canonical bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContestPackSignature {
    pub key_id: String,
    pub algorithm: PackSignatureAlgorithm,
    pub digest: String,
    pub signature: String,
}

/// A pack as it is distributed: content plus an optional detached signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContestPackEnvelope {
    pub pack: ContestDefinitionPack,
    #[serde(default)]
    pub signature: Option<ContestPackSignature>,
}

/// The signing keys a deployment trusts for contest definition updates.
#[derive(Debug, Clone, Default)]
pub struct ContestPackTrustStore {
    keys: HashMap<String, Vec<u8>>,
    allow_unsigned: bool,
}

impl ContestPackTrustStore {
    /// A trust store that only accepts packs signed by a known key.
    pub fn new() -> Self {
        Self {
            keys: HashMap::new(),
            allow_unsigned: false,
        }
    }

    /// A trust store that also accepts unsigned packs.
    ///
    /// This exists for the built-in pack and for tests; production update
    /// paths use [`ContestPackTrustStore::new`].
    pub fn allowing_unsigned() -> Self {
        Self {
            keys: HashMap::new(),
            allow_unsigned: true,
        }
    }

    pub fn with_key(mut self, key_id: impl Into<String>, secret: impl Into<Vec<u8>>) -> Self {
        self.keys.insert(key_id.into(), secret.into());
        self
    }

    pub fn trusts_unsigned(&self) -> bool {
        self.allow_unsigned
    }

    fn verify(
        &self,
        pack: &ContestDefinitionPack,
        signature: Option<&ContestPackSignature>,
    ) -> Result<(), ContestSchemaError> {
        let Some(signature) = signature else {
            return if self.allow_unsigned {
                Ok(())
            } else {
                Err(ContestSchemaError::SignatureRequired)
            };
        };

        let secret = self
            .keys
            .get(&signature.key_id)
            .ok_or_else(|| ContestSchemaError::UnknownSigningKey(signature.key_id.clone()))?;

        let canonical = pack.canonical_bytes();
        let mut hasher = Sha256::new();
        hasher.update(&canonical);
        if !constant_time_eq(
            hex_lower(&hasher.finalize()).as_bytes(),
            signature.digest.as_bytes(),
        ) {
            return Err(ContestSchemaError::DigestMismatch);
        }

        let expected = sign_canonical_bytes(secret, &canonical);
        if !constant_time_eq(expected.as_bytes(), signature.signature.as_bytes()) {
            return Err(ContestSchemaError::SignatureInvalid);
        }
        Ok(())
    }
}

/// Produce the signature value for a pack's canonical bytes.
pub fn sign_canonical_bytes(secret: &[u8], canonical: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(canonical);
    hex_lower(&mac.finalize().into_bytes())
}

/// Build a detached signature for a pack.
pub fn sign_definition_pack(
    pack: &ContestDefinitionPack,
    key_id: impl Into<String>,
    secret: &[u8],
) -> ContestPackSignature {
    let canonical = pack.canonical_bytes();
    let mut hasher = Sha256::new();
    hasher.update(&canonical);
    ContestPackSignature {
        key_id: key_id.into(),
        algorithm: PackSignatureAlgorithm::HmacSha256,
        digest: hex_lower(&hasher.finalize()),
        signature: sign_canonical_bytes(secret, &canonical),
    }
}

/// Load a bare (unsigned) definition pack.
///
/// The pack is rejected whole if its kind, schema version, or any definition
/// is not understood; a partially loaded pack is never returned.
pub fn load_definition_pack(json: &str) -> Result<ContestDefinitionPack, ContestSchemaError> {
    let pack: ContestDefinitionPack = serde_json::from_str(json)
        .map_err(|error| ContestSchemaError::InvalidJson(error.to_string()))?;
    pack.validate()?;
    Ok(pack)
}

/// Load a distributed pack envelope, verifying the signature first.
pub fn load_signed_definition_pack(
    json: &str,
    trust: &ContestPackTrustStore,
) -> Result<ContestDefinitionPack, ContestSchemaError> {
    let envelope: ContestPackEnvelope = serde_json::from_str(json)
        .map_err(|error| ContestSchemaError::InvalidJson(error.to_string()))?;
    trust.verify(&envelope.pack, envelope.signature.as_ref())?;
    envelope.pack.validate()?;
    Ok(envelope.pack)
}

/// The definitions compiled into this build.
pub fn builtin_definition_pack() -> ContestDefinitionPack {
    load_definition_pack(BUILTIN_PACK_JSON)
        .expect("the built-in contest definition pack matches the supported schema")
}

/// Where a catalog entry came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContestDefinitionOrigin {
    /// Compiled into the application build.
    Builtin,
    /// Installed at runtime from the named pack.
    Pack { pack_id: String, pack_version: u32 },
}

/// One resolved contest definition and its provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContestCatalogEntry {
    pub definition: ContestDefinition,
    pub origin: ContestDefinitionOrigin,
}

/// The definitions available to the application right now.
///
/// A catalog starts from the built-in pack and takes installed packs on top.
/// For each contest the highest `rule_version` wins, so a corrected definition
/// can be delivered without an application release, and an older pack cannot
/// silently downgrade a contest that has already been updated.
#[derive(Debug, Clone, Default)]
pub struct ContestDefinitionCatalog {
    entries: BTreeMap<String, ContestCatalogEntry>,
}

impl ContestDefinitionCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// A catalog seeded with the built-in definitions.
    pub fn with_builtin() -> Self {
        let mut catalog = Self::new();
        catalog.install(&builtin_definition_pack(), ContestDefinitionOrigin::Builtin);
        catalog
    }

    /// Apply a pack, keeping the highest rule version for each contest.
    ///
    /// Returns the contest ids this pack actually changed.
    pub fn install_pack(&mut self, pack: &ContestDefinitionPack) -> Vec<String> {
        let origin = ContestDefinitionOrigin::Pack {
            pack_id: pack.pack_id.clone(),
            pack_version: pack.pack_version,
        };
        self.install(pack, origin)
    }

    fn install(
        &mut self,
        pack: &ContestDefinitionPack,
        origin: ContestDefinitionOrigin,
    ) -> Vec<String> {
        let mut applied = Vec::new();
        for definition in &pack.definitions {
            let replace = self
                .entries
                .get(&definition.contest_id)
                .is_none_or(|entry| definition.rule_version > entry.definition.rule_version);
            if replace {
                applied.push(definition.contest_id.clone());
                self.entries.insert(
                    definition.contest_id.clone(),
                    ContestCatalogEntry {
                        definition: definition.clone(),
                        origin: origin.clone(),
                    },
                );
            }
        }
        applied
    }

    pub fn get(&self, contest_id: &str) -> Result<&ContestCatalogEntry, ContestSchemaError> {
        self.entries
            .get(contest_id)
            .ok_or_else(|| ContestSchemaError::UnknownContest(contest_id.to_owned()))
    }

    pub fn definition(&self, contest_id: &str) -> Result<&ContestDefinition, ContestSchemaError> {
        self.get(contest_id).map(|entry| &entry.definition)
    }

    /// Every entry, ordered by contest id.
    pub fn entries(&self) -> impl Iterator<Item = &ContestCatalogEntry> {
        self.entries.values()
    }

    pub fn contest_ids(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Definitions running at `at`, ordered by contest id.
    pub fn active_at(&self, at: DateTime<Utc>) -> Vec<&ContestCatalogEntry> {
        self.entries
            .values()
            .filter(|entry| entry.definition.is_active_at(at))
            .collect()
    }

    /// A redacted summary suitable for runtime events and support bundles.
    pub fn summary(&self) -> Value {
        let definitions = self
            .entries
            .values()
            .map(|entry| {
                serde_json::json!({
                    "contest_id": entry.definition.contest_id,
                    "name": entry.definition.name,
                    "rule_version": entry.definition.rule_version,
                    "origin": entry.origin,
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "schema_version": CONTEST_RULE_SCHEMA_VERSION,
            "definition_count": definitions.len(),
            "definitions": definitions,
        })
    }
}

fn normalize_exchange_value(
    field: &ExchangeField,
    value: &str,
) -> Result<String, ContestSchemaError> {
    let invalid = |reason: String| ContestSchemaError::InvalidExchangeField {
        key: field.key.clone(),
        reason,
    };

    match &field.kind {
        ExchangeFieldKind::Serial => {
            let parsed: u32 = value
                .parse()
                .map_err(|_| invalid("a serial must be a whole number".to_owned()))?;
            if parsed == 0 {
                return Err(invalid("a serial must be greater than zero".to_owned()));
            }
            Ok(parsed.to_string())
        }
        ExchangeFieldKind::Rst => {
            let upper = value.to_ascii_uppercase();
            let usable = upper.len() >= 2
                && upper.len() <= 4
                && upper
                    .chars()
                    .all(|character| character.is_ascii_digit() || character == 'A');
            if usable {
                Ok(upper)
            } else {
                Err(invalid(
                    "a signal report must be 2 to 4 report characters".to_owned(),
                ))
            }
        }
        ExchangeFieldKind::Callsign => {
            let upper = value.to_ascii_uppercase();
            let usable = upper.len() >= 3
                && upper
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '/');
            if usable {
                Ok(upper)
            } else {
                Err(invalid(
                    "a callsign must be alphanumeric with optional `/`".to_owned(),
                ))
            }
        }
        ExchangeFieldKind::Grid => {
            let normalized = normalize_grid(value)
                .ok_or_else(|| invalid("a grid square must be 4, 6, or 8 characters".to_owned()))?;
            Ok(normalized)
        }
        ExchangeFieldKind::Text { max_len } => {
            if value.chars().count() > *max_len {
                return Err(invalid(format!(
                    "text must be {max_len} characters or fewer"
                )));
            }
            Ok(value.to_owned())
        }
        ExchangeFieldKind::Integer { min, max } => {
            let parsed: i64 = value
                .parse()
                .map_err(|_| invalid("a whole number is required".to_owned()))?;
            if parsed < *min || parsed > *max {
                return Err(invalid(format!("value must be between {min} and {max}")));
            }
            Ok(parsed.to_string())
        }
        ExchangeFieldKind::Choice { options } => options
            .iter()
            .find(|option| option.eq_ignore_ascii_case(value))
            .cloned()
            .ok_or_else(|| invalid(format!("value must be one of: {}", options.join(", ")))),
    }
}

fn normalize_grid(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if !matches!(trimmed.len(), 4 | 6 | 8) {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let mut normalized = String::with_capacity(trimmed.len());
    for (index, byte) in bytes.iter().enumerate() {
        let character = *byte as char;
        let ok = match index {
            0 | 1 => character.is_ascii_alphabetic(),
            2 | 3 => character.is_ascii_digit(),
            4 | 5 => character.is_ascii_alphabetic(),
            _ => character.is_ascii_digit(),
        };
        if !ok {
            return None;
        }
        match index {
            0 | 1 => normalized.push(character.to_ascii_uppercase()),
            4 | 5 => normalized.push(character.to_ascii_lowercase()),
            _ => normalized.push(character),
        }
    }
    Some(normalized)
}

fn matches_any(allowed: &[String], value: &str) -> bool {
    allowed.is_empty()
        || allowed
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(value.trim()))
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right.iter())
        .fold(0u8, |accumulator, (a, b)| accumulator | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pack_json(schema_version: u32) -> String {
        serde_json::json!({
            "kind": CONTEST_DEFINITION_PACK_KIND,
            "schema_version": schema_version,
            "pack_id": "test.pack",
            "pack_version": 1,
            "generated_at": "2026-08-31T00:00:00Z",
            "definitions": [sample_definition_json(1)],
        })
        .to_string()
    }

    fn sample_definition_json(rule_version: u32) -> Value {
        serde_json::json!({
            "contest_id": "test-serial",
            "name": "Test Serial Contest",
            "sponsor": "Test Sponsor",
            "rule_version": rule_version,
            "rule_source": "test fixture",
            "bands": ["20m", "40m"],
            "modes": ["CW", "PH"],
            "exchange": {
                "sent": [
                    { "key": "rst_sent", "label": "RST Sent", "kind": { "type": "rst" } },
                    { "key": "serial_sent", "label": "Serial Sent", "kind": { "type": "serial" } }
                ],
                "received": [
                    { "key": "rst_received", "label": "RST Received", "kind": { "type": "rst" } },
                    { "key": "serial_received", "label": "Serial Received", "kind": { "type": "serial" } },
                    { "key": "section", "label": "Section", "kind": { "type": "choice", "options": ["OH", "MI"] }, "required": false }
                ]
            },
            "duplicate_rule": { "scope": "per_band_mode" },
            "serial_policy": "per_contest",
            "multipliers": [
                {
                    "key": "section",
                    "label": "Sections",
                    "source": { "type": "received_exchange_field", "field_key": "section" },
                    "scope": "per_contest"
                }
            ],
            "scoring": { "default_points": 1, "rules": [{ "bands": ["40m"], "modes": ["CW"], "points": 2 }] },
            "export": { "cabrillo_name": "TEST-SERIAL", "cabrillo_version": "3.0" }
        })
    }

    fn exchange(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn builtin_pack_matches_the_supported_schema() {
        let pack = builtin_definition_pack();
        assert_eq!(pack.schema_version, CONTEST_RULE_SCHEMA_VERSION);
        assert_eq!(pack.kind, CONTEST_DEFINITION_PACK_KIND);
        assert!(pack.definition("generic-serial").is_some());
        assert!(pack.definition("generic-grid").is_some());
    }

    #[test]
    fn newer_schema_versions_are_rejected_whole() {
        let error = load_definition_pack(&sample_pack_json(CONTEST_RULE_SCHEMA_VERSION + 1))
            .expect_err("a newer schema must not load");
        assert_eq!(
            error,
            ContestSchemaError::UnsupportedSchemaVersion {
                found: CONTEST_RULE_SCHEMA_VERSION + 1,
                supported: CONTEST_RULE_SCHEMA_VERSION,
            }
        );
    }

    #[test]
    fn unknown_pack_kinds_are_rejected() {
        let mut value: Value = serde_json::from_str(&sample_pack_json(1)).expect("valid fixture");
        value["kind"] = serde_json::json!("someone.elses.pack");
        let error = load_definition_pack(&value.to_string()).expect_err("kind must be checked");
        assert!(matches!(
            error,
            ContestSchemaError::UnsupportedPackKind { .. }
        ));
    }

    #[test]
    fn unknown_definition_fields_fail_instead_of_being_ignored() {
        let mut value: Value = serde_json::from_str(&sample_pack_json(1)).expect("valid fixture");
        value["definitions"][0]["bonus_station_rule"] = serde_json::json!({ "points": 100 });
        let error =
            load_definition_pack(&value.to_string()).expect_err("unknown rules must not load");
        assert!(matches!(error, ContestSchemaError::InvalidJson(_)));
    }

    #[test]
    fn definitions_that_contradict_their_serial_policy_are_rejected() {
        let mut value: Value = serde_json::from_str(&sample_pack_json(1)).expect("valid fixture");
        value["definitions"][0]["serial_policy"] = serde_json::json!("none");
        let error = load_definition_pack(&value.to_string()).expect_err("policy must agree");
        assert!(matches!(
            error,
            ContestSchemaError::InvalidDefinition { .. }
        ));
    }

    #[test]
    fn multipliers_must_reference_a_declared_received_field() {
        let mut value: Value = serde_json::from_str(&sample_pack_json(1)).expect("valid fixture");
        value["definitions"][0]["multipliers"][0]["source"]["field_key"] =
            serde_json::json!("not_declared");
        let error = load_definition_pack(&value.to_string()).expect_err("multiplier must resolve");
        assert!(matches!(
            error,
            ContestSchemaError::InvalidDefinition { .. }
        ));
    }

    #[test]
    fn repeated_contest_ids_are_rejected() {
        let mut value: Value = serde_json::from_str(&sample_pack_json(1)).expect("valid fixture");
        value["definitions"]
            .as_array_mut()
            .expect("definitions array")
            .push(sample_definition_json(2));
        let error = load_definition_pack(&value.to_string()).expect_err("ids must be unique");
        assert_eq!(
            error,
            ContestSchemaError::DuplicateContestId("test-serial".to_owned())
        );
    }

    #[test]
    fn signed_packs_load_and_tampered_packs_do_not() {
        let pack = load_definition_pack(&sample_pack_json(1)).expect("valid fixture");
        let signature = sign_definition_pack(&pack, "release-2026", b"contest-signing-secret");
        let envelope = ContestPackEnvelope {
            pack: pack.clone(),
            signature: Some(signature),
        };
        let trust = ContestPackTrustStore::new()
            .with_key("release-2026", b"contest-signing-secret".to_vec());

        let json = serde_json::to_string(&envelope).expect("envelope serializes");
        let loaded = load_signed_definition_pack(&json, &trust).expect("signed pack loads");
        assert_eq!(loaded.pack_id, "test.pack");

        let mut tampered = envelope.clone();
        tampered.pack.definitions[0].scoring.default_points = 1_000;
        let json = serde_json::to_string(&tampered).expect("envelope serializes");
        assert_eq!(
            load_signed_definition_pack(&json, &trust).expect_err("tampering must be caught"),
            ContestSchemaError::DigestMismatch
        );
    }

    #[test]
    fn unsigned_and_unknown_key_packs_are_refused_by_default() {
        let pack = load_definition_pack(&sample_pack_json(1)).expect("valid fixture");
        let trust = ContestPackTrustStore::new().with_key("release-2026", b"secret".to_vec());

        let unsigned = ContestPackEnvelope {
            pack: pack.clone(),
            signature: None,
        };
        let json = serde_json::to_string(&unsigned).expect("envelope serializes");
        assert_eq!(
            load_signed_definition_pack(&json, &trust).expect_err("unsigned must be refused"),
            ContestSchemaError::SignatureRequired
        );

        let stranger = ContestPackEnvelope {
            pack,
            signature: Some(sign_definition_pack(
                &load_definition_pack(&sample_pack_json(1)).expect("valid fixture"),
                "someone-else",
                b"secret",
            )),
        };
        let json = serde_json::to_string(&stranger).expect("envelope serializes");
        assert_eq!(
            load_signed_definition_pack(&json, &trust).expect_err("unknown key must be refused"),
            ContestSchemaError::UnknownSigningKey("someone-else".to_owned())
        );
    }

    #[test]
    fn unsigned_packs_load_only_where_they_are_explicitly_allowed() {
        let pack = load_definition_pack(&sample_pack_json(1)).expect("valid fixture");
        let json = serde_json::to_string(&ContestPackEnvelope {
            pack,
            signature: None,
        })
        .expect("envelope serializes");
        let trust = ContestPackTrustStore::allowing_unsigned();
        assert!(trust.trusts_unsigned());
        assert!(load_signed_definition_pack(&json, &trust).is_ok());
    }

    #[test]
    fn a_newer_rule_version_replaces_an_older_one_without_an_app_release() {
        let mut catalog = ContestDefinitionCatalog::with_builtin();
        assert_eq!(
            catalog
                .definition("generic-serial")
                .expect("builtin definition")
                .rule_version,
            1
        );

        let mut value: Value = serde_json::from_str(
            &serde_json::json!({
                "kind": CONTEST_DEFINITION_PACK_KIND,
                "schema_version": CONTEST_RULE_SCHEMA_VERSION,
                "pack_id": "ke8ygw.update.2027",
                "pack_version": 2,
                "generated_at": "2027-01-01T00:00:00Z",
                "definitions": [],
            })
            .to_string(),
        )
        .expect("update pack skeleton");
        let mut updated = builtin_definition_pack()
            .definition("generic-serial")
            .cloned()
            .expect("builtin definition");
        updated.rule_version = 2;
        updated.scoring.default_points = 3;
        value["definitions"] = serde_json::json!([updated]);

        let update = load_definition_pack(&value.to_string()).expect("update pack loads");
        assert_eq!(catalog.install_pack(&update), vec!["generic-serial"]);

        let entry = catalog.get("generic-serial").expect("updated definition");
        assert_eq!(entry.definition.rule_version, 2);
        assert_eq!(entry.definition.scoring.default_points, 3);
        assert_eq!(
            entry.origin,
            ContestDefinitionOrigin::Pack {
                pack_id: "ke8ygw.update.2027".to_owned(),
                pack_version: 2,
            }
        );
    }

    #[test]
    fn an_older_pack_cannot_downgrade_an_updated_contest() {
        let mut catalog = ContestDefinitionCatalog::new();
        let newer = load_definition_pack(
            &serde_json::json!({
                "kind": CONTEST_DEFINITION_PACK_KIND,
                "schema_version": CONTEST_RULE_SCHEMA_VERSION,
                "pack_id": "newer",
                "pack_version": 2,
                "generated_at": "2027-01-01T00:00:00Z",
                "definitions": [sample_definition_json(4)],
            })
            .to_string(),
        )
        .expect("newer pack");
        let older = load_definition_pack(&sample_pack_json(CONTEST_RULE_SCHEMA_VERSION))
            .expect("older pack");

        assert_eq!(catalog.install_pack(&newer), vec!["test-serial"]);
        assert!(catalog.install_pack(&older).is_empty());
        assert_eq!(
            catalog
                .definition("test-serial")
                .expect("definition")
                .rule_version,
            4
        );
    }

    #[test]
    fn unknown_contests_are_reported_rather_than_guessed() {
        let catalog = ContestDefinitionCatalog::with_builtin();
        assert_eq!(
            catalog.definition("field-day").expect_err("not installed"),
            ContestSchemaError::UnknownContest("field-day".to_owned())
        );
    }

    #[test]
    fn exchange_values_are_validated_and_normalized() {
        let pack = load_definition_pack(&sample_pack_json(1)).expect("valid fixture");
        let definition = pack.definition("test-serial").expect("definition");

        let normalized = definition
            .validate_exchange(
                ExchangeDirection::Received,
                &exchange(&[
                    ("rst_received", "59"),
                    ("serial_received", "007"),
                    ("section", "oh"),
                ]),
            )
            .expect("valid exchange");
        assert_eq!(
            normalized.get("serial_received").map(String::as_str),
            Some("7")
        );
        assert_eq!(normalized.get("section").map(String::as_str), Some("OH"));

        let error = definition
            .validate_exchange(
                ExchangeDirection::Received,
                &exchange(&[("rst_received", "59")]),
            )
            .expect_err("serial is required");
        assert_eq!(
            error,
            ContestSchemaError::MissingExchangeField {
                key: "serial_received".to_owned()
            }
        );

        let error = definition
            .validate_exchange(
                ExchangeDirection::Received,
                &exchange(&[
                    ("rst_received", "59"),
                    ("serial_received", "7"),
                    ("section", "ZZ"),
                ]),
            )
            .expect_err("section must be a known option");
        assert!(matches!(
            error,
            ContestSchemaError::InvalidExchangeField { .. }
        ));

        let error = definition
            .validate_exchange(
                ExchangeDirection::Received,
                &exchange(&[
                    ("rst_received", "59"),
                    ("serial_received", "7"),
                    ("power", "100"),
                ]),
            )
            .expect_err("unknown fields must not be accepted");
        assert_eq!(
            error,
            ContestSchemaError::UnknownExchangeField {
                key: "power".to_owned()
            }
        );
    }

    #[test]
    fn grid_exchange_values_are_normalized_to_standard_casing() {
        let catalog = ContestDefinitionCatalog::with_builtin();
        let definition = catalog.definition("generic-grid").expect("definition");
        let normalized = definition
            .validate_exchange(
                ExchangeDirection::Received,
                &exchange(&[("rst_received", "59"), ("grid_received", "en91WM")]),
            )
            .expect("valid grid exchange");
        assert_eq!(
            normalized.get("grid_received").map(String::as_str),
            Some("EN91wm")
        );

        let error = definition
            .validate_exchange(
                ExchangeDirection::Received,
                &exchange(&[("rst_received", "59"), ("grid_received", "EN9")]),
            )
            .expect_err("a three character grid is not valid");
        assert!(matches!(
            error,
            ContestSchemaError::InvalidExchangeField { .. }
        ));
    }

    #[test]
    fn duplicate_scope_decides_the_comparison_key() {
        let per_band_mode = DuplicateRule {
            scope: DuplicateScope::PerBandMode,
            repeat_after_minutes: None,
        };
        assert_eq!(
            per_band_mode.duplicate_key("w1aw", "20m", "cw"),
            "W1AW|20M|CW"
        );
        assert_ne!(
            per_band_mode.duplicate_key("W1AW", "20m", "CW"),
            per_band_mode.duplicate_key("W1AW", "20m", "PH")
        );

        let per_contest = DuplicateRule {
            scope: DuplicateScope::PerContest,
            repeat_after_minutes: None,
        };
        assert_eq!(
            per_contest.duplicate_key("W1AW", "20m", "CW"),
            per_contest.duplicate_key("w1aw", "40m", "PH")
        );
    }

    #[test]
    fn serial_sequences_follow_the_declared_policy() {
        assert_eq!(SerialPolicy::None.sequence_key("20m", "CW"), None);
        assert_eq!(
            SerialPolicy::PerContest.sequence_key("20m", "CW"),
            Some("contest".to_owned())
        );
        assert_eq!(
            SerialPolicy::PerBand.sequence_key("20m", "CW"),
            SerialPolicy::PerBand.sequence_key("20M", "PH")
        );
        assert_ne!(
            SerialPolicy::PerBandMode.sequence_key("20m", "CW"),
            SerialPolicy::PerBandMode.sequence_key("20m", "PH")
        );
    }

    #[test]
    fn the_first_matching_scoring_rule_wins() {
        let pack = load_definition_pack(&sample_pack_json(1)).expect("valid fixture");
        let definition = pack.definition("test-serial").expect("definition");
        assert_eq!(definition.scoring.points_for("40m", "CW"), 2);
        assert_eq!(definition.scoring.points_for("40m", "PH"), 1);
        assert_eq!(definition.scoring.points_for("20m", "CW"), 1);
    }

    #[test]
    fn time_windows_gate_availability_and_generic_templates_stay_open() {
        let mut pack = load_definition_pack(&sample_pack_json(1)).expect("valid fixture");
        let inside = DateTime::parse_from_rfc3339("2026-11-24T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        let outside = DateTime::parse_from_rfc3339("2026-12-24T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);

        assert!(pack.definitions[0].is_active_at(outside));
        pack.definitions[0].time_windows = vec![ContestTimeWindow {
            starts_at: DateTime::parse_from_rfc3339("2026-11-24T00:00:00Z")
                .expect("timestamp")
                .with_timezone(&Utc),
            ends_at: DateTime::parse_from_rfc3339("2026-11-25T00:00:00Z")
                .expect("timestamp")
                .with_timezone(&Utc),
        }];
        assert!(pack.definitions[0].is_active_at(inside));
        assert!(!pack.definitions[0].is_active_at(outside));
    }

    #[test]
    fn inverted_time_windows_are_rejected() {
        let mut value: Value = serde_json::from_str(&sample_pack_json(1)).expect("valid fixture");
        value["definitions"][0]["time_windows"] = serde_json::json!([{
            "starts_at": "2026-11-25T00:00:00Z",
            "ends_at": "2026-11-24T00:00:00Z",
        }]);
        let error = load_definition_pack(&value.to_string()).expect_err("window must be ordered");
        assert!(matches!(
            error,
            ContestSchemaError::InvalidDefinition { .. }
        ));
    }

    #[test]
    fn the_catalog_summary_reports_versions_and_provenance() {
        let catalog = ContestDefinitionCatalog::with_builtin();
        let summary = catalog.summary();
        assert_eq!(summary["schema_version"], CONTEST_RULE_SCHEMA_VERSION);
        assert_eq!(summary["definition_count"], catalog.len());
        assert_eq!(summary["definitions"][0]["origin"], "builtin");
        assert!(!catalog.is_empty());
        assert!(catalog.contest_ids().contains(&"generic-grid".to_owned()));
        assert_eq!(
            catalog.active_at(Utc::now()).len(),
            catalog.entries().count()
        );
    }

    #[test]
    fn pack_digests_change_when_content_changes() {
        let pack = load_definition_pack(&sample_pack_json(1)).expect("valid fixture");
        let mut changed = pack.clone();
        changed.definitions[0].scoring.default_points = 5;
        assert_ne!(pack.digest(), changed.digest());
        assert_eq!(pack.digest(), pack.clone().digest());
    }
}
