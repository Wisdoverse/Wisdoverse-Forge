//! `run_migrations` moves a legacy-shaped `public.team_invites` out of the way
//! of migration 089 (#1356) and leaves every other database untouched.

use sqlx::PgPool;
use sqlx::migrate::Migrator;
use uuid::Uuid;

/// Apply only the migrations up to and including `version`.
async fn migrate_up_to(pool: &PgPool, version: i64) {
    let all = sqlx::migrate!("./migrations");
    let subset: Vec<_> = all.migrations.iter().filter(|m| m.version <= version).cloned().collect();
    Migrator { migrations: subset.into(), ..sqlx::migrate!("./migrations") }.run(pool).await.unwrap();
}

fn latest_version() -> i64 {
    sqlx::migrate!("./migrations").migrations.iter().map(|m| m.version).max().unwrap()
}

/// Legacy backend shape: no `token_hash`; `serial` id creates
/// `team_invites_id_seq`, and the PK index uses the name 089 also creates.
async fn create_legacy_table(pool: &PgPool, schema: &str, rows: usize) {
    sqlx::raw_sql(&format!(
        "CREATE TABLE {schema}.team_invites (
             id serial PRIMARY KEY,
             team_id text NOT NULL,
             email varchar(255) NOT NULL,
             role varchar(10) NOT NULL DEFAULT 'member',
             token varchar(64) NOT NULL UNIQUE,
             invited_by text,
             created_at timestamptz NOT NULL DEFAULT now()
         )"
    ))
    .execute(pool)
    .await
    .unwrap();
    for i in 0..rows {
        sqlx::query(&format!("INSERT INTO {schema}.team_invites (team_id, email, token) VALUES ('t1', $1, $2)"))
            .bind(format!("user{i}@example.com"))
            .bind(Uuid::new_v4().simple().to_string())
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM {table}")).fetch_one(pool).await.unwrap()
}

async fn has_column(pool: &PgPool, schema: &str, column: &str) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns
                         WHERE table_schema = $1 AND table_name = 'team_invites' AND column_name = $2)",
    )
    .bind(schema)
    .bind(column)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn ledger_has(pool: &PgPool, version: i64) -> bool {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM _sqlx_migrations WHERE version = $1)")
        .bind(version)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Tables in the schemas reconciliation can write to.
async fn reconciled_tables(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT schemaname || '.' || tablename FROM pg_tables
          WHERE schemaname = 'legacy' OR schemaname LIKE 'legacy\\_pre\\_%' ORDER BY 1",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = false)]
async fn legacy_table_at_ledger_88_is_moved_and_migrations_apply(pool: PgPool) {
    migrate_up_to(&pool, 88).await;
    create_legacy_table(&pool, "public", 2).await;
    // A table referencing the legacy one keeps its foreign key to the moved table.
    sqlx::raw_sql(
        "CREATE TABLE public.legacy_invite_notes (invite_id int NOT NULL REFERENCES team_invites(id));
         INSERT INTO public.legacy_invite_notes SELECT min(id) FROM team_invites;",
    )
    .execute(&pool)
    .await
    .unwrap();

    agentforge_db::run_migrations(&pool).await.unwrap();

    assert_eq!(count(&pool, "legacy_pre_089.team_invites").await, 2, "legacy rows are preserved");
    assert!(has_column(&pool, "legacy_pre_089", "token").await);
    assert!(has_column(&pool, "public", "token_hash").await, "089 created the current shape");
    assert_eq!(count(&pool, "public.team_invites").await, 0);
    assert!(ledger_has(&pool, 89).await);
    assert!(ledger_has(&pool, latest_version()).await);
    let owners: Vec<(String, String)> = sqlx::query_as(
        "SELECT conname::text, conrelid::regclass::text FROM pg_constraint
          WHERE conname IN ('team_invites_pkey', 'team_invites_team_id_email_key') ORDER BY 1, 2",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        owners,
        [
            ("team_invites_pkey".to_owned(), "legacy_pre_089.team_invites".to_owned()),
            ("team_invites_pkey".to_owned(), "team_invites".to_owned()),
            ("team_invites_team_id_email_key".to_owned(), "team_invites".to_owned()),
        ],
        "089 constraint names are free in public once the legacy table moved"
    );
    let fk_target: String = sqlx::query_scalar(
        "SELECT confrelid::regclass::text FROM pg_constraint
          WHERE conrelid = 'public.legacy_invite_notes'::regclass AND contype = 'f'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(fk_target, "legacy_pre_089.team_invites", "foreign keys follow the moved table");
    assert_eq!(count(&pool, "public.legacy_invite_notes").await, 1);

    // Second run is a no-op.
    agentforge_db::run_migrations(&pool).await.unwrap();
    assert_eq!(reconciled_tables(&pool).await, ["legacy_pre_089.team_invites"]);
    assert_eq!(count(&pool, "legacy_pre_089.team_invites").await, 2);
    assert_eq!(count(&pool, "public.team_invites").await, 0);
}

#[sqlx::test(migrations = false)]
async fn unrelated_legacy_schema_copy_is_left_alone(pool: PgPool) {
    migrate_up_to(&pool, 88).await;
    // Migration 027 dropped `legacy`; an operator may have recreated it.
    sqlx::raw_sql("CREATE SCHEMA legacy").execute(&pool).await.unwrap();
    create_legacy_table(&pool, "legacy", 1).await;
    create_legacy_table(&pool, "public", 2).await;

    agentforge_db::run_migrations(&pool).await.unwrap();

    assert_eq!(reconciled_tables(&pool).await, ["legacy.team_invites", "legacy_pre_089.team_invites"]);
    assert_eq!(count(&pool, "legacy.team_invites").await, 1);
    assert_eq!(count(&pool, "legacy_pre_089.team_invites").await, 2);
    assert!(has_column(&pool, "public", "token_hash").await);
    assert!(ledger_has(&pool, latest_version()).await);
}

#[sqlx::test(migrations = false)]
async fn taken_target_is_refused_without_changes(pool: PgPool) {
    migrate_up_to(&pool, 88).await;
    sqlx::raw_sql("CREATE SCHEMA legacy_pre_089").execute(&pool).await.unwrap();
    create_legacy_table(&pool, "legacy_pre_089", 1).await;
    create_legacy_table(&pool, "public", 2).await;

    let err = agentforge_db::run_migrations(&pool).await.unwrap_err();

    assert!(matches!(err, agentforge_db::RunMigrationsError::LegacyTargetExists { .. }), "{err}");
    assert!(err.to_string().contains("legacy_pre_089.team_invites already exists"), "{err}");
    assert_eq!(count(&pool, "legacy_pre_089.team_invites").await, 1);
    assert_eq!(count(&pool, "public.team_invites").await, 2);
    assert!(has_column(&pool, "public", "token").await, "public table untouched");
    assert!(!ledger_has(&pool, 89).await, "migrator did not run");
}

/// No ledger yet: reconciliation runs before 000..027; 027 drops `legacy`
/// with CASCADE but never touches `legacy_pre_089`.
#[sqlx::test(migrations = false)]
async fn legacy_table_without_ledger_survives_migration_027(pool: PgPool) {
    create_legacy_table(&pool, "public", 2).await;

    agentforge_db::run_migrations(&pool).await.unwrap();

    assert_eq!(reconciled_tables(&pool).await, ["legacy_pre_089.team_invites"]);
    assert_eq!(count(&pool, "legacy_pre_089.team_invites").await, 2);
    assert!(has_column(&pool, "public", "token_hash").await);
    assert!(ledger_has(&pool, latest_version()).await);
}

#[sqlx::test(migrations = false)]
async fn fresh_database_has_no_reconciliation_side_effects(pool: PgPool) {
    agentforge_db::run_migrations(&pool).await.unwrap();

    assert!(reconciled_tables(&pool).await.is_empty());
    let extra_schema: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname LIKE 'legacy\\_pre\\_%')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!extra_schema);
    assert!(has_column(&pool, "public", "token_hash").await);
}

#[sqlx::test(migrations = "./migrations")]
async fn current_table_past_089_is_untouched(pool: PgPool) {
    let (org, team, user) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Invite Org', $2)")
        .bind(org)
        .bind(org.to_string())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, 'dev@example.com')")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO teams (id, organization_id, name, slug) VALUES ($1, $2, 'Team', $3)")
        .bind(team)
        .bind(org)
        .bind(team.to_string())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO team_invites (organization_id, team_id, email, token_hash, created_by, expires_at)
         VALUES ($1, $2, 'invitee@example.com', 'hash', $3, now() + interval '1 day')",
    )
    .bind(org)
    .bind(team)
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    let oid_before: i64 =
        sqlx::query_scalar("SELECT 'public.team_invites'::regclass::oid::bigint").fetch_one(&pool).await.unwrap();

    agentforge_db::run_migrations(&pool).await.unwrap();

    let oid_after: i64 =
        sqlx::query_scalar("SELECT 'public.team_invites'::regclass::oid::bigint").fetch_one(&pool).await.unwrap();
    assert_eq!(oid_before, oid_after, "same table, not moved or recreated");
    assert_eq!(count(&pool, "public.team_invites").await, 1);
    assert!(reconciled_tables(&pool).await.is_empty());
}

#[sqlx::test(migrations = false)]
async fn hand_created_current_shape_without_ledger_entry_is_not_moved(pool: PgPool) {
    migrate_up_to(&pool, 88).await;
    sqlx::raw_sql(include_str!("../migrations/089_team_invites.sql")).execute(&pool).await.unwrap();
    let oid_before: i64 =
        sqlx::query_scalar("SELECT 'public.team_invites'::regclass::oid::bigint").fetch_one(&pool).await.unwrap();
    assert!(!ledger_has(&pool, 89).await);

    agentforge_db::run_migrations(&pool).await.unwrap();

    let oid_after: i64 =
        sqlx::query_scalar("SELECT 'public.team_invites'::regclass::oid::bigint").fetch_one(&pool).await.unwrap();
    assert_eq!(oid_before, oid_after);
    assert!(reconciled_tables(&pool).await.is_empty());
    assert!(ledger_has(&pool, 89).await);
}
