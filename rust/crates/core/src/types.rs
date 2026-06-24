//! Domain ID types and common enums.
//!
//! Strongly-typed ID wrappers using the newtype pattern prevent accidentally
//! mixing up identifiers from different domains (e.g. passing an `OrgId` where
//! an `AgentId` is expected).

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Helper macro to define a newtype UUID wrapper with standard derives and impls.
macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
        #[sqlx(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Create a new ID using UUID v7 (time-sortable).
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            /// Access the underlying UUID.
            pub fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl From<Uuid> for $name {
            fn from(uuid: Uuid) -> Self {
                Self(uuid)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

define_id!(
    /// Unique identifier for a managed coding agent (container).
    AgentId
);
define_id!(
    /// Unique identifier for an organization (tenant).
    OrgId
);
define_id!(
    /// Unique identifier for a user.
    UserId
);
define_id!(
    /// Unique identifier for a project.
    ProjectId
);
define_id!(
    /// Unique identifier for a workspace.
    WorkspaceId
);
define_id!(
    /// Unique identifier for a team.
    TeamId
);
define_id!(
    /// Unique identifier for an event.
    EventId
);
define_id!(
    /// Unique identifier for a setting.
    SettingId
);
define_id!(
    /// Unique identifier for a feature flag.
    FeatureFlagId
);
define_id!(
    /// Unique identifier for a favorite.
    FavoriteId
);
define_id!(
    /// Unique identifier for an audit log entry.
    AuditLogId
);
define_id!(
    /// Unique identifier for a group.
    GroupId
);
define_id!(
    /// Unique identifier for a plugin.
    PluginId
);
define_id!(
    /// Unique identifier for a skill.
    SkillId
);
define_id!(
    /// Unique identifier for a governed memory item.
    MemoryItemId
);
define_id!(
    /// Unique identifier for an analytics event.
    AnalyticsEventId
);
define_id!(
    /// Unique identifier for a license.
    LicenseId
);
define_id!(
    /// Unique identifier for a voice provider.
    VoiceProviderId
);
define_id!(
    /// Unique identifier for a dashboard tile.
    TileId
);
define_id!(
    /// Unique identifier for a saved prompt.
    PromptId
);
define_id!(
    /// Unique identifier for an agent chat message (issue #21 provider+prompt).
    MessageId
);
define_id!(
    /// Unique identifier for a file attachment.
    AttachmentId
);
define_id!(
    /// Unique identifier for a resource profile.
    ResourceProfileId
);
define_id!(
    /// Unique identifier for a quota usage record.
    QuotaUsageId
);
define_id!(
    /// Unique identifier for a dev environment.
    DevEnvironmentId
);

/// Agent status enum matching the state machine.
///
/// Transitions:
/// - `user_prompt_submit` / `pre_tool_use` → Working
/// - `stop` / `session_end` → Idle
/// - Container dies / no activity 2min → Offline / Idle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "agent_status", rename_all = "lowercase")]
pub enum AgentStatus {
    Working,
    Idle,
    Offline,
}

impl fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentStatus::Working => write!(f, "working"),
            AgentStatus::Idle => write!(f, "idle"),
            AgentStatus::Offline => write!(f, "offline"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_creation_is_unique() {
        let a = AgentId::new();
        let b = AgentId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn id_display_matches_inner_uuid() {
        let id = AgentId::new();
        assert_eq!(id.to_string(), id.as_uuid().to_string());
    }

    #[test]
    fn id_serialization_roundtrip() {
        let id = OrgId::new();
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: OrgId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    #[test]
    fn different_id_types_are_distinct() {
        // This is a compile-time guarantee, but we verify the values are independent
        let uuid = Uuid::now_v7();
        let agent_id = AgentId::from(uuid);
        let org_id = OrgId::from(uuid);
        // Same inner UUID, but different types — cannot be compared at compile time
        assert_eq!(agent_id.as_uuid(), org_id.as_uuid());
    }

    #[test]
    fn agent_status_display() {
        assert_eq!(AgentStatus::Working.to_string(), "working");
        assert_eq!(AgentStatus::Idle.to_string(), "idle");
        assert_eq!(AgentStatus::Offline.to_string(), "offline");
    }

    #[test]
    fn agent_status_serialization() {
        let json = serde_json::to_string(&AgentStatus::Working).unwrap();
        assert_eq!(json, "\"working\"");
        let deserialized: AgentStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, AgentStatus::Working);
    }
}
