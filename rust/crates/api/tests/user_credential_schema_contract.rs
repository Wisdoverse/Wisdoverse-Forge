use std::collections::BTreeMap;

use sqlx::PgPool;

#[derive(Debug)]
struct Column {
    name: String,
    data_type: String,
    max_length: Option<i32>,
    nullable: String,
    default: Option<String>,
}

async fn columns(pool: &PgPool, table: &str) -> BTreeMap<String, Column> {
    sqlx::query_as::<_, (String, String, Option<i32>, String, Option<String>)>(
        r#"
        SELECT column_name, data_type, character_maximum_length, is_nullable, column_default
          FROM information_schema.columns
         WHERE table_name = $1
         ORDER BY ordinal_position
        "#,
    )
    .bind(table)
    .fetch_all(pool)
    .await
    .expect("load columns")
    .into_iter()
    .map(|(name, data_type, max_length, nullable, default)| {
        (name.clone(), Column { name, data_type, max_length, nullable, default })
    })
    .collect()
}

async fn constraint_defs(pool: &PgPool, table: &str) -> BTreeMap<String, String> {
    sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT conname, pg_get_constraintdef(oid)
          FROM pg_constraint
         WHERE conrelid = $1::regclass
         ORDER BY conname
        "#,
    )
    .bind(table)
    .fetch_all(pool)
    .await
    .expect("load constraints")
    .into_iter()
    .collect()
}

async fn index_defs(pool: &PgPool, table: &str) -> BTreeMap<String, String> {
    sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT indexname, indexdef
          FROM pg_indexes
         WHERE tablename = $1
         ORDER BY indexname
        "#,
    )
    .bind(table)
    .fetch_all(pool)
    .await
    .expect("load indexes")
    .into_iter()
    .collect()
}

fn expect_col<'a>(columns: &'a BTreeMap<String, Column>, name: &str) -> &'a Column {
    columns.get(name).unwrap_or_else(|| panic!("missing column {name}; got {:?}", columns.keys().collect::<Vec<_>>()))
}

fn expect_default_contains(col: &Column, needle: &str) {
    let default = col.default.as_deref().unwrap_or_else(|| panic!("missing default for {}", col.name));
    assert!(default.contains(needle), "default for {} should contain {needle:?}, got {default:?}", col.name);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn user_cli_credentials_schema_matches_rust_owned_contract(pool: PgPool) {
    let cols = columns(&pool, "user_cli_credentials").await;

    let id = expect_col(&cols, "id");
    assert_eq!(id.data_type, "uuid");
    assert_eq!(id.nullable, "NO");
    expect_default_contains(id, "gen_random_uuid");

    assert_eq!(expect_col(&cols, "user_id").data_type, "uuid");

    let cli_tool = expect_col(&cols, "cli_tool");
    assert_eq!(cli_tool.data_type, "character varying");
    assert_eq!(cli_tool.max_length, Some(20));
    assert_eq!(cli_tool.nullable, "NO");

    assert_eq!(expect_col(&cols, "encrypted_credentials").data_type, "text");
    assert_eq!(expect_col(&cols, "revoked_at").data_type, "timestamp with time zone");
    assert_eq!(expect_col(&cols, "revoke_reason").data_type, "text");
    assert_eq!(expect_col(&cols, "refresh_fail_count").data_type, "integer");
    assert_eq!(expect_col(&cols, "last_refresh_error").data_type, "text");
    assert_eq!(expect_col(&cols, "last_refresh_error_at").data_type, "timestamp with time zone");

    let constraints = constraint_defs(&pool, "user_cli_credentials").await;
    assert_eq!(constraints.get("user_cli_credentials_pkey").map(String::as_str), Some("PRIMARY KEY (id)"));
    assert_eq!(
        constraints.get("user_cli_credentials_user_id_cli_tool_key").map(String::as_str),
        Some("UNIQUE (user_id, cli_tool)")
    );
    assert_eq!(
        constraints.get("user_cli_credentials_user_id_fkey").map(String::as_str),
        Some("FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE")
    );

    let indexes = index_defs(&pool, "user_cli_credentials").await;
    assert!(indexes.contains_key("idx_user_cli_credentials_active"));
    assert!(indexes.contains_key("idx_user_cli_credentials_user"));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn user_llm_configs_schema_matches_rust_owned_contract(pool: PgPool) {
    let cols = columns(&pool, "user_llm_configs").await;

    let id = expect_col(&cols, "id");
    assert_eq!(id.data_type, "uuid");
    assert_eq!(id.nullable, "NO");
    expect_default_contains(id, "gen_random_uuid");

    let provider = expect_col(&cols, "provider");
    assert_eq!(provider.data_type, "character varying");
    assert_eq!(provider.max_length, Some(50));
    assert_eq!(provider.nullable, "NO");

    let model = expect_col(&cols, "model");
    assert_eq!(model.data_type, "character varying");
    assert_eq!(model.max_length, Some(100));
    assert_eq!(model.nullable, "YES");

    let display_name = expect_col(&cols, "display_name");
    assert_eq!(display_name.data_type, "character varying");
    assert_eq!(display_name.max_length, Some(100));

    assert_eq!(expect_col(&cols, "base_url").data_type, "text");
    assert_eq!(expect_col(&cols, "encrypted_api_key").data_type, "text");

    let api_key_prefix = expect_col(&cols, "api_key_prefix");
    assert_eq!(api_key_prefix.data_type, "character varying");
    assert_eq!(api_key_prefix.max_length, Some(20));

    let settings = expect_col(&cols, "settings");
    assert_eq!(settings.data_type, "jsonb");
    expect_default_contains(settings, "'{}'::jsonb");

    let constraints = constraint_defs(&pool, "user_llm_configs").await;
    assert_eq!(constraints.get("user_llm_configs_pkey").map(String::as_str), Some("PRIMARY KEY (id)"));
    assert_eq!(
        constraints.get("user_llm_configs_user_id_fkey").map(String::as_str),
        Some("FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE")
    );

    let indexes = index_defs(&pool, "user_llm_configs").await;
    assert!(indexes.contains_key("idx_user_llm_configs_default"));
    assert!(indexes.contains_key("idx_user_llm_configs_user"));
    assert!(indexes.contains_key("idx_user_llm_configs_user_provider_enabled"));
    assert!(indexes.contains_key("uq_user_llm_provider_model"));
    assert!(indexes.contains_key("uq_user_llm_provider_no_model"));
}
