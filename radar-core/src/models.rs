use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// The kind of change detected between two spec versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    FieldRemoved,
    FieldAdded,
    TypeChanged,
    RequiredChanged,
    OperationRemoved,
    OperationAdded,
    ParameterRemoved,
    ResponseRemoved,
    EnumValueRemoved,
    EnumValueAdded,
    NullabilityChanged,
    RequestBodyAdded,
    RequestBodyRemoved,
}

impl ChangeKind {
    /// Returns the canonical snake_case string for this kind (matches the serde serialisation).
    pub fn as_str(&self) -> &'static str {
        match self {
            ChangeKind::FieldRemoved => "field_removed",
            ChangeKind::FieldAdded => "field_added",
            ChangeKind::TypeChanged => "type_changed",
            ChangeKind::RequiredChanged => "required_changed",
            ChangeKind::OperationRemoved => "operation_removed",
            ChangeKind::OperationAdded => "operation_added",
            ChangeKind::ParameterRemoved => "parameter_removed",
            ChangeKind::ResponseRemoved => "response_removed",
            ChangeKind::EnumValueRemoved => "enum_value_removed",
            ChangeKind::EnumValueAdded => "enum_value_added",
            ChangeKind::NullabilityChanged => "nullability_changed",
            ChangeKind::RequestBodyAdded => "request_body_added",
            ChangeKind::RequestBodyRemoved => "request_body_removed",
        }
    }
}

/// How severe a change is for consumers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Least severe — safe to deploy without coordination.
    Safe,
    /// Medium severity — risky but not guaranteed to break consumers.
    NonBreakingRisky,
    /// Most severe — will break consumers that depend on the changed element.
    Breaking,
}

/// How confident the scanner is about a blast-radius entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// The wire format / schema language used by a service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecFormat {
    OpenApi,
    GraphQL,
    Protobuf,
}

// ---------------------------------------------------------------------------
// Core domain structs
// ---------------------------------------------------------------------------

/// A single detected change within a diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub id: Uuid,
    pub diff_id: Uuid,
    /// JSON-pointer-style path to the changed element (e.g. `/paths/~1users/get`).
    pub path: String,
    pub kind: ChangeKind,
    pub severity: Severity,
    pub description: Option<String>,
}

/// A captured snapshot of a service's spec at a particular git ref.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecVersion {
    pub id: Uuid,
    pub service_id: Uuid,
    pub git_ref: String,
    pub captured_at: DateTime<Utc>,
    pub spec_format: SpecFormat,
}

/// A service whose API spec is tracked by drift monitor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub id: Uuid,
    pub name: String,
    pub repo_url: String,
    pub owner_team: String,
    pub spec_format: SpecFormat,
}

/// A downstream team / application that consumes one or more services.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Consumer {
    pub id: Uuid,
    pub name: String,
    pub repo_url: String,
    pub owner_team: String,
    /// Primary contact (e.g. email or Slack handle).
    pub contact: String,
}

/// A relationship indicating that a consumer depends on a service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: Uuid,
    pub service_id: Uuid,
    pub consumer_id: Uuid,
    pub opted_in_at: DateTime<Utc>,
}

/// The result of diffing two spec versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diff {
    pub id: Uuid,
    pub from_version: Uuid,
    pub to_version: Uuid,
    /// URL of the pull request that introduced this diff, if known.
    pub pr_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// One row in the blast-radius report: a consumer likely affected by a diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastEntry {
    pub consumer: Consumer,
    pub confidence: Confidence,
    pub last_seen: DateTime<Utc>,
}
