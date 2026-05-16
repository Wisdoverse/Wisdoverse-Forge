//! Platform configuration domain rules.
//!
//! This module owns pure validation and normalization policies for
//! operator-managed configuration surfaces such as quotas, resource profiles,
//! dashboard tiles, and plugin catalog entries.

use agentforge_core::{AppResult, ErrorKind};
use uuid::Uuid;

const VALID_QUOTA_RESOURCE_TYPES: &[&str] = &["agents", "storage", "events"];
const VALID_TILE_TYPES: &[&str] = &["agent", "feed", "chart", "custom"];
const MAX_RESOURCE_PROFILE_NAME_LEN: usize = 100;
const MAX_PLUGIN_NAME_LEN: usize = 255;
const DEFAULT_PLUGIN_VERSION: &str = "0.1.0";

/// Quota resource type tracked by the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct QuotaResourceType<'a> {
    value: &'a str,
}

impl<'a> QuotaResourceType<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        if !VALID_QUOTA_RESOURCE_TYPES.contains(&value) {
            return Err(ErrorKind::Validation(format!(
                "resource_type must be one of: {:?}",
                VALID_QUOTA_RESOURCE_TYPES
            ))
            .into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Resource profile policy for container runtime limits.
pub(crate) struct ResourceProfilePolicy;

impl ResourceProfilePolicy {
    pub(crate) fn validate_create(
        name: &str,
        cpu_millicores: i32,
        memory_mb: i32,
        storage_mb: i32,
        max_pids: i32,
    ) -> AppResult<()> {
        Self::validate_name(name)?;
        Self::validate_positive("cpu_millicores", cpu_millicores)?;
        Self::validate_positive("memory_mb", memory_mb)?;
        Self::validate_positive("storage_mb", storage_mb)?;
        Self::validate_positive("max_pids", max_pids)
    }

    pub(crate) fn validate_update(
        name: Option<&str>,
        cpu_millicores: Option<i32>,
        memory_mb: Option<i32>,
        storage_mb: Option<i32>,
        max_pids: Option<i32>,
    ) -> AppResult<()> {
        if let Some(name) = name {
            Self::validate_name(name)?;
        }
        if let Some(value) = cpu_millicores {
            Self::validate_positive("cpu_millicores", value)?;
        }
        if let Some(value) = memory_mb {
            Self::validate_positive("memory_mb", value)?;
        }
        if let Some(value) = storage_mb {
            Self::validate_positive("storage_mb", value)?;
        }
        if let Some(value) = max_pids {
            Self::validate_positive("max_pids", value)?;
        }
        Ok(())
    }

    fn validate_name(name: &str) -> AppResult<()> {
        if name.is_empty() || name.len() > MAX_RESOURCE_PROFILE_NAME_LEN {
            return Err(
                ErrorKind::Validation(format!("name must be 1-{MAX_RESOURCE_PROFILE_NAME_LEN} characters")).into()
            );
        }
        Ok(())
    }

    fn validate_positive(field: &str, value: i32) -> AppResult<()> {
        if value <= 0 {
            return Err(ErrorKind::Validation(format!("{field} must be positive")).into());
        }
        Ok(())
    }
}

/// Dashboard tile type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TileType<'a> {
    value: &'a str,
}

impl<'a> TileType<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        if !VALID_TILE_TYPES.contains(&value) {
            return Err(ErrorKind::Validation(format!("tile_type must be one of: {:?}", VALID_TILE_TYPES)).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Dashboard tile layout policy.
pub(crate) struct TileLayoutPolicy;

impl TileLayoutPolicy {
    pub(crate) fn validate_dimensions(width: i32, height: i32) -> AppResult<()> {
        if width < 1 || height < 1 {
            return Err(ErrorKind::Validation("width and height must be >= 1".into()).into());
        }
        Ok(())
    }

    pub(crate) fn validate_width(width: i32) -> AppResult<()> {
        if width < 1 {
            return Err(ErrorKind::Validation("width must be >= 1".into()).into());
        }
        Ok(())
    }

    pub(crate) fn validate_height(height: i32) -> AppResult<()> {
        if height < 1 {
            return Err(ErrorKind::Validation("height must be >= 1".into()).into());
        }
        Ok(())
    }

    pub(crate) fn validate_bulk_layout(tiles: &[(Uuid, i32, i32, i32, i32)]) -> AppResult<()> {
        if tiles.is_empty() {
            return Err(ErrorKind::Validation("tiles array must not be empty".into()).into());
        }
        for &(_, _, _, width, height) in tiles {
            Self::validate_dimensions(width, height)?;
        }
        Ok(())
    }
}

/// Plugin catalog display name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PluginName<'a> {
    value: &'a str,
}

impl<'a> PluginName<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        let value = value.trim();
        if value.is_empty() || value.len() > MAX_PLUGIN_NAME_LEN {
            return Err(ErrorKind::Validation("plugin name must be 1-255 characters".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Plugin catalog version defaulting policy.
pub(crate) struct PluginVersion<'a> {
    value: &'a str,
}

impl<'a> PluginVersion<'a> {
    pub(crate) fn from_optional(value: Option<&'a str>) -> Self {
        Self { value: value.unwrap_or(DEFAULT_PLUGIN_VERSION) }
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_resource_type_accepts_known_resources() {
        assert_eq!(QuotaResourceType::parse("agents").unwrap().value(), "agents");
        assert_eq!(QuotaResourceType::parse("storage").unwrap().value(), "storage");
        assert_eq!(QuotaResourceType::parse("events").unwrap().value(), "events");
    }

    #[test]
    fn quota_resource_type_rejects_unknown_resources() {
        assert!(QuotaResourceType::parse("cpu").is_err());
        assert!(QuotaResourceType::parse("").is_err());
    }

    #[test]
    fn resource_profile_create_policy_matches_existing_bounds() {
        assert!(ResourceProfilePolicy::validate_create("small", 1000, 512, 2048, 128).is_ok());
        assert!(ResourceProfilePolicy::validate_create("", 1000, 512, 2048, 128).is_err());
        assert!(ResourceProfilePolicy::validate_create(&"a".repeat(101), 1000, 512, 2048, 128).is_err());
        assert!(ResourceProfilePolicy::validate_create("small", 0, 512, 2048, 128).is_err());
        assert!(ResourceProfilePolicy::validate_create("small", 1000, 0, 2048, 128).is_err());
        assert!(ResourceProfilePolicy::validate_create("small", 1000, 512, 0, 128).is_err());
        assert!(ResourceProfilePolicy::validate_create("small", 1000, 512, 2048, 0).is_err());
    }

    #[test]
    fn resource_profile_update_policy_allows_partial_updates() {
        assert!(ResourceProfilePolicy::validate_update(None, None, None, None, None).is_ok());
        assert!(ResourceProfilePolicy::validate_update(Some("medium"), Some(1000), None, None, None).is_ok());
        assert!(ResourceProfilePolicy::validate_update(Some(""), None, None, None, None).is_err());
        assert!(ResourceProfilePolicy::validate_update(None, Some(-1), None, None, None).is_err());
    }

    #[test]
    fn tile_type_accepts_supported_surfaces() {
        assert_eq!(TileType::parse("agent").unwrap().value(), "agent");
        assert_eq!(TileType::parse("feed").unwrap().value(), "feed");
        assert_eq!(TileType::parse("chart").unwrap().value(), "chart");
        assert_eq!(TileType::parse("custom").unwrap().value(), "custom");
    }

    #[test]
    fn tile_type_rejects_unknown_surfaces() {
        assert!(TileType::parse("widget").is_err());
        assert!(TileType::parse("").is_err());
    }

    #[test]
    fn tile_layout_policy_preserves_dimension_rules() {
        assert!(TileLayoutPolicy::validate_dimensions(1, 1).is_ok());
        assert!(TileLayoutPolicy::validate_dimensions(0, 1).is_err());
        assert!(TileLayoutPolicy::validate_width(0).is_err());
        assert!(TileLayoutPolicy::validate_height(0).is_err());
    }

    #[test]
    fn tile_layout_policy_rejects_empty_bulk_updates() {
        assert!(TileLayoutPolicy::validate_bulk_layout(&[]).is_err());
    }

    #[test]
    fn plugin_name_policy_trims_and_bounds_names() {
        assert_eq!(PluginName::parse(" my-plugin ").unwrap().value(), "my-plugin");
        assert!(PluginName::parse("").is_err());
        assert!(PluginName::parse(&"a".repeat(256)).is_err());
    }

    #[test]
    fn plugin_version_defaults_to_existing_version() {
        assert_eq!(PluginVersion::from_optional(None).value(), "0.1.0");
        assert_eq!(PluginVersion::from_optional(Some("1.2.3")).value(), "1.2.3");
    }
}
