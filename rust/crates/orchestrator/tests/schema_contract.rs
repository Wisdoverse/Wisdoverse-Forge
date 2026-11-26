use sqlx::{PgPool, Row};

async fn column_udt(pool: &PgPool, table: &str, column: &str) -> String {
    sqlx::query_scalar(
        "SELECT udt_name
           FROM information_schema.columns
          WHERE table_schema = 'public'
            AND table_name = $1
            AND column_name = $2",
    )
    .bind(table)
    .bind(column)
    .fetch_one(pool)
    .await
    .expect("column exists")
}

async fn legacy_table_exists(pool: &PgPool, table: &str) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1
              FROM information_schema.tables
             WHERE table_schema = 'legacy_orchestrator'
               AND table_name = $1
        )",
    )
    .bind(table)
    .fetch_one(pool)
    .await
    .expect("legacy table existence query")
}

/// #1352 reconciliation columns on `workflows`, with their expected types.
async fn assert_reconcile_columns(pool: &PgPool) {
    for (column, udt) in [
        ("temporal_namespace_id", "text"),
        ("status_cause", "text"),
        ("execution_missing_since", "timestamptz"),
        ("execution_checked_at", "timestamptz"),
        ("execution_observation", "text"),
        ("reconciled_at", "timestamptz"),
        ("state_version", "int8"),
    ] {
        assert_eq!(column_udt(pool, "workflows", column).await, udt, "workflows.{column}");
    }
}

async fn version_of(pool: &PgPool, id: &str) -> (i64, chrono::DateTime<chrono::Utc>) {
    sqlx::query_as("SELECT state_version, updated_at FROM workflows WHERE id = CAST($1 AS uuid)")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("read version")
}

/// #1352 trigger contract on `workflows`: a bookkeeping-only update moves
/// neither `updated_at` nor `state_version`; any other update bumps both, and
/// two real updates in one transaction get two different versions.
async fn assert_state_version_trigger(pool: &PgPool, id: &str) {
    let start = version_of(pool, id).await;
    sqlx::query(
        "UPDATE workflows SET execution_checked_at = NOW(), execution_missing_since = NOW(),
                              execution_observation = 'missing' WHERE id = CAST($1 AS uuid)",
    )
    .bind(id)
    .execute(pool)
    .await
    .expect("bookkeeping update");
    assert_eq!(version_of(pool, id).await, start, "bookkeeping-only update must change neither column");

    sqlx::query("UPDATE workflows SET description = 'changed' WHERE id = CAST($1 AS uuid)")
        .bind(id)
        .execute(pool)
        .await
        .expect("real update");
    let after_real = version_of(pool, id).await;
    assert_eq!(after_real.0, start.0 + 1, "a real update increments state_version");
    assert!(after_real.1 > start.1, "a real update sets updated_at");

    // A real update that also touches bookkeeping still counts as real.
    sqlx::query("UPDATE workflows SET status = 'failed', execution_checked_at = NOW() WHERE id = CAST($1 AS uuid)")
        .bind(id)
        .execute(pool)
        .await
        .expect("mixed update");
    let after_mixed = version_of(pool, id).await;
    assert_eq!(after_mixed.0, after_real.0 + 1, "status plus bookkeeping is a real update");

    let mut tx = pool.begin().await.expect("begin");
    let mut versions = Vec::new();
    for name in ["first", "second"] {
        let version: i64 = sqlx::query_scalar(
            "UPDATE workflows SET description = $2 WHERE id = CAST($1 AS uuid) RETURNING state_version",
        )
        .bind(id)
        .bind(name)
        .fetch_one(&mut *tx)
        .await
        .expect("update in transaction");
        versions.push(version);
    }
    tx.commit().await.expect("commit");
    assert_ne!(versions[0], versions[1], "two changes in one transaction share now() but not a version");
}

async fn seed_workflow(pool: &PgPool) -> String {
    let participant: String = sqlx::query_scalar(
        "INSERT INTO participants (type, display_name, casdoor_user_id, org_id)
         VALUES ('human', 'schema-user', 'casdoor-schema-' || gen_random_uuid(), 'org-schema') RETURNING id::text",
    )
    .fetch_one(pool)
    .await
    .expect("seed participant");
    sqlx::query_scalar(
        "INSERT INTO workflows (name, status, org_id, created_by)
         VALUES ('schema-wf', 'running', 'org-schema', CAST($1 AS uuid)) RETURNING id::text",
    )
    .bind(participant)
    .fetch_one(pool)
    .await
    .expect("seed workflow")
}

#[sqlx::test(migrations = "./migrations")]
async fn fresh_schema_matches_rust_owned_uuid_contract(pool: PgPool) {
    for (table, column) in [
        ("tasks", "id"),
        ("tasks", "workflow_id"),
        ("tasks", "created_by"),
        ("workflows", "id"),
        ("workflows", "created_by"),
        ("workflow_nodes", "id"),
        ("workflow_nodes", "workflow_id"),
        ("workflow_node_dependencies", "node_id"),
        ("code_reviews", "id"),
        ("code_reviews", "task_id"),
        ("review_comments", "id"),
        ("review_comments", "review_id"),
        ("knowledge_entries", "id"),
        ("knowledge_entries", "created_by"),
        ("audit_logs", "id"),
    ] {
        assert_eq!(column_udt(&pool, table, column).await, "uuid", "{table}.{column}");
    }
    assert_reconcile_columns(&pool).await;
    let id = seed_workflow(&pool).await;
    assert_state_version_trigger(&pool, &id).await;
}

#[sqlx::test(migrations = false)]
async fn upgrade_from_013_adds_state_version_trigger(pool: PgPool) {
    let mut up_to_013 = sqlx::migrate!("./migrations");
    up_to_013.migrations =
        std::borrow::Cow::Owned(up_to_013.migrations.iter().filter(|m| m.version <= 13).cloned().collect());
    up_to_013.run(&pool).await.expect("migrate to 013");
    let id = seed_workflow(&pool).await;

    sqlx::migrate!("./migrations").run(&pool).await.expect("migrate 013 -> 014");

    assert_reconcile_columns(&pool).await;
    assert_eq!(version_of(&pool, &id).await.0, 0, "existing rows start at version 0");
    assert_state_version_trigger(&pool, &id).await;
}

#[sqlx::test(migrations = false)]
async fn legacy_integer_schema_is_preserved_and_replaced_with_uuid_tables(pool: PgPool) {
    sqlx::raw_sql(
        r#"
        CREATE TABLE workflows (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'draft',
            org_id TEXT NOT NULL,
            created_by TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE TABLE workflow_nodes (
            id SERIAL PRIMARY KEY,
            workflow_id INTEGER NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            type TEXT NOT NULL,
            config JSONB NOT NULL DEFAULT '{}',
            position INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE workflow_node_dependencies (
            node_id INTEGER NOT NULL REFERENCES workflow_nodes(id) ON DELETE CASCADE,
            depends_on INTEGER NOT NULL REFERENCES workflow_nodes(id) ON DELETE CASCADE,
            PRIMARY KEY (node_id, depends_on)
        );

        CREATE TABLE tasks (
            id SERIAL PRIMARY KEY,
            workflow_id INTEGER REFERENCES workflows(id),
            title TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            state TEXT NOT NULL DEFAULT 'pending',
            priority TEXT NOT NULL DEFAULT 'normal',
            assigned_to TEXT,
            review_id INTEGER,
            created_by TEXT NOT NULL,
            org_id TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            CONSTRAINT tasks_state_check
                CHECK (state IN ('pending', 'assigned', 'working', 'review', 'completed', 'failed'))
        );

        CREATE TABLE task_dependencies (
            task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            depends_on INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            PRIMARY KEY (task_id, depends_on)
        );

        CREATE TABLE code_reviews (
            id SERIAL PRIMARY KEY,
            task_id INTEGER NOT NULL REFERENCES tasks(id),
            session_id TEXT NOT NULL,
            diff_ref TEXT NOT NULL,
            state TEXT NOT NULL DEFAULT 'pending',
            assigned_to TEXT,
            org_id TEXT NOT NULL,
            created_by TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE TABLE review_comments (
            id SERIAL PRIMARY KEY,
            review_id INTEGER NOT NULL REFERENCES code_reviews(id) ON DELETE CASCADE,
            author_id TEXT NOT NULL,
            body TEXT NOT NULL,
            file_path TEXT,
            line INTEGER,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE TABLE knowledge_entries (
            id SERIAL PRIMARY KEY,
            type TEXT NOT NULL,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            source_id TEXT,
            tags TEXT[] NOT NULL DEFAULT '{}',
            org_id TEXT NOT NULL,
            created_by TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            CONSTRAINT knowledge_entries_type_check
                CHECK (type IN ('session', 'document', 'snippet'))
        );

        CREATE TABLE audit_logs (
            id SERIAL PRIMARY KEY,
            action TEXT NOT NULL,
            actor_id TEXT NOT NULL,
            actor_type TEXT NOT NULL,
            resource TEXT NOT NULL,
            resource_id TEXT,
            org_id TEXT NOT NULL,
            changes JSONB,
            ip_address INET,
            user_agent TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        INSERT INTO workflows (name, org_id, created_by) VALUES ('legacy', 'org-1', 'user-1');
        INSERT INTO workflow_nodes (workflow_id, name, type) VALUES (1, 'legacy-node', 'gate');
        "#,
    )
    .execute(&pool)
    .await
    .expect("seed legacy integer schema");

    sqlx::migrate!("./migrations").run(&pool).await.expect("run orchestrator migrations");
    assert_reconcile_columns(&pool).await;

    for (table, column) in [
        ("workflows", "id"),
        ("workflow_nodes", "workflow_id"),
        ("tasks", "id"),
        ("tasks", "workflow_id"),
        ("code_reviews", "task_id"),
        ("knowledge_entries", "id"),
        ("audit_logs", "id"),
    ] {
        assert_eq!(column_udt(&pool, table, column).await, "uuid", "{table}.{column}");
    }

    for legacy_table in [
        "workflows_legacy_int",
        "workflow_nodes_legacy_int",
        "tasks_legacy_int",
        "code_reviews_legacy_int",
        "knowledge_entries_legacy_int",
        "audit_logs_legacy_int",
    ] {
        assert!(legacy_table_exists(&pool, legacy_table).await, "{legacy_table}");
    }

    let row = sqlx::query("SELECT id, name FROM legacy_orchestrator.workflows_legacy_int WHERE id = 1")
        .fetch_one(&pool)
        .await
        .expect("legacy workflow row retained");
    let id: i32 = row.get("id");
    let name: String = row.get("name");
    assert_eq!(id, 1);
    assert_eq!(name, "legacy");
}
