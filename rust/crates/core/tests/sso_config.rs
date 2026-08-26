//! SSO config contract: env mapping and fail-fast validation.

use agentforge_core::AppConfig;

#[test]
fn sso_enabled_without_required_fields_fails_fast() {
    temp_env::with_vars(
        [
            ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
            ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
            ("AUTH_SSO__ENABLED", Some("true")),
            ("AUTH_SSO__OIDC_DISCOVERY_URL", None),
            ("AUTH_SSO__OIDC_CLIENT_ID", None),
            ("AUTH_SSO__OIDC_CLIENT_SECRET", None),
            ("AUTH_SSO__SPA_BASE_URL", None),
        ],
        || {
            let err = AppConfig::from_env().expect_err("SSO enabled without required fields must fail");
            let message = err.to_string();
            assert!(message.contains("AUTH_SSO__OIDC_DISCOVERY_URL"), "error was: {message}");
            assert!(message.contains("AUTH_SSO__OIDC_CLIENT_ID"), "error was: {message}");
            assert!(message.contains("AUTH_SSO__OIDC_CLIENT_SECRET"), "error was: {message}");
            assert!(message.contains("AUTH_SSO__SPA_BASE_URL"), "error was: {message}");
            assert!(message.contains("AUTH_SSO__ENABLED requires"), "error was: {message}");
        },
    );
}

#[test]
fn sso_completed_config_loads_with_defaults() {
    temp_env::with_vars(
        [
            ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
            ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
            ("AUTH_SSO__ENABLED", Some("true")),
            ("AUTH_SSO__OIDC_DISCOVERY_URL", Some("https://sso.example.com/.well-known/openid-configuration")),
            ("AUTH_SSO__OIDC_CLIENT_ID", Some("forge")),
            ("AUTH_SSO__OIDC_CLIENT_SECRET", Some("top-secret")),
            ("AUTH_SSO__SPA_BASE_URL", Some("https://forge.example.com")),
            ("AUTH_SSO__DISPLAY_NAME", Some("Company sign-in")),
        ],
        || {
            let cfg = AppConfig::from_env().expect("completed SSO config should load");
            assert!(cfg.auth_sso.enabled);
            assert_eq!(cfg.auth_sso.display_name.as_deref(), Some("Company sign-in"));
            assert_eq!(cfg.auth_sso.oidc_scopes, "openid profile email");
        },
    );
}

#[test]
fn sso_role_mapping_requires_both_role_claim_and_admin_groups() {
    // Only one half configured: the mapping contract must be explicit.
    temp_env::with_vars(
        [
            ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
            ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
            ("AUTH_SSO__ENABLED", Some("true")),
            ("AUTH_SSO__OIDC_DISCOVERY_URL", Some("https://sso.example.com/.well-known/openid-configuration")),
            ("AUTH_SSO__OIDC_CLIENT_ID", Some("forge")),
            ("AUTH_SSO__OIDC_CLIENT_SECRET", Some("top-secret")),
            ("AUTH_SSO__SPA_BASE_URL", Some("https://forge.example.com")),
            ("AUTH_SSO__ROLE_CLAIM", Some("groups")),
            ("AUTH_SSO__ADMIN_GROUPS", None),
        ],
        || {
            let err = AppConfig::from_env().expect_err("partial role mapping must fail");
            let message = err.to_string();
            assert!(message.contains("AUTH_SSO__ADMIN_GROUPS"), "error was: {message}");
        },
    );

    // Complete mapping loads.
    temp_env::with_vars(
        [
            ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
            ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
            ("AUTH_SSO__ENABLED", Some("true")),
            ("AUTH_SSO__OIDC_DISCOVERY_URL", Some("https://sso.example.com/.well-known/openid-configuration")),
            ("AUTH_SSO__OIDC_CLIENT_ID", Some("forge")),
            ("AUTH_SSO__OIDC_CLIENT_SECRET", Some("top-secret")),
            ("AUTH_SSO__SPA_BASE_URL", Some("https://forge.example.com")),
            ("AUTH_SSO__ROLE_CLAIM", Some("groups")),
            ("AUTH_SSO__ADMIN_GROUPS", Some("forge-admins, admins")),
        ],
        || {
            let cfg = AppConfig::from_env().expect("complete role mapping should load");
            assert_eq!(cfg.auth_sso.role_claim.as_deref(), Some("groups"));
            assert_eq!(cfg.auth_sso.admin_groups.as_deref(), Some("forge-admins, admins"));
        },
    );
}

#[test]
fn sso_org_provisioning_requires_role_claim_and_map_for_deprovision() {
    // Org map without the groups claim would have no groups to match on.
    temp_env::with_vars(
        [
            ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
            ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
            ("AUTH_SSO__ENABLED", Some("true")),
            ("AUTH_SSO__OIDC_DISCOVERY_URL", Some("https://sso.example.com/.well-known/openid-configuration")),
            ("AUTH_SSO__OIDC_CLIENT_ID", Some("forge")),
            ("AUTH_SSO__OIDC_CLIENT_SECRET", Some("top-secret")),
            ("AUTH_SSO__SPA_BASE_URL", Some("https://forge.example.com")),
            ("AUTH_SSO__ORG_GROUP_MAP", Some("team-org=team-apps")),
        ],
        || {
            let err = AppConfig::from_env().expect_err("org map without role claim must fail");
            let message = err.to_string();
            assert!(message.contains("AUTH_SSO__ROLE_CLAIM"), "error was: {message}");
        },
    );

    // Deprovisioning without a map would remove nothing; it must fail fast.
    temp_env::with_vars(
        [
            ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
            ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
            ("AUTH_SSO__ENABLED", Some("true")),
            ("AUTH_SSO__OIDC_DISCOVERY_URL", Some("https://sso.example.com/.well-known/openid-configuration")),
            ("AUTH_SSO__OIDC_CLIENT_ID", Some("forge")),
            ("AUTH_SSO__OIDC_CLIENT_SECRET", Some("top-secret")),
            ("AUTH_SSO__SPA_BASE_URL", Some("https://forge.example.com")),
            ("AUTH_SSO__ROLE_CLAIM", Some("groups")),
            ("AUTH_SSO__DEPROVISION", Some("true")),
        ],
        || {
            let err = AppConfig::from_env().expect_err("deprovision without org map must fail");
            let message = err.to_string();
            assert!(message.contains("AUTH_SSO__ORG_GROUP_MAP"), "error was: {message}");
        },
    );
}

#[test]
fn sso_team_provisioning_requires_role_claim() {
    // Team map without the groups claim would have no groups to match on.
    temp_env::with_vars(
        [
            ("DATABASE_URL", Some("postgres://localhost/agentforge_test")),
            ("JWT_SECRET", Some("test-secret-key-min-32-chars-long!!")),
            ("AUTH_SSO__ENABLED", Some("true")),
            ("AUTH_SSO__OIDC_DISCOVERY_URL", Some("https://sso.example.com/.well-known/openid-configuration")),
            ("AUTH_SSO__OIDC_CLIENT_ID", Some("forge")),
            ("AUTH_SSO__OIDC_CLIENT_SECRET", Some("top-secret")),
            ("AUTH_SSO__SPA_BASE_URL", Some("https://forge.example.com")),
            ("AUTH_SSO__TEAM_GROUP_MAP", Some("Builders=team-apps")),
        ],
        || {
            let err = AppConfig::from_env().expect_err("team map without role claim must fail");
            let message = err.to_string();
            assert!(message.contains("AUTH_SSO__ROLE_CLAIM"), "error was: {message}");
        },
    );
}
