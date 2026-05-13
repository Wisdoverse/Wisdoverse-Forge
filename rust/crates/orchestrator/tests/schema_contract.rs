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
