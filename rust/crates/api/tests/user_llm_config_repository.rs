use agentforge_api::repositories::user::llm_config::UserLlmConfigRepository;
use sqlx::PgPool;

mod common;

#[sqlx::test(migrations = "../db/migrations")]
async fn find_default_secret_returns_base_url_and_preserves_api_key_lookup(pool: PgPool) {
    let user_id = common::seed_user(&pool).await;
    let scope = common::scope_for(user_id);
    let repo = UserLlmConfigRepository::new(pool.clone());

    sqlx::query(
        r#"INSERT INTO user_llm_configs
              (user_id, provider, model, encrypted_api_key, base_url, is_enabled, is_default, settings)
           VALUES
              ($1, 'groq', 'old-model', 'cipher-old', NULL, TRUE, FALSE, '{}'::jsonb),
              ($1, 'groq', 'new-model', 'cipher-new', 'https://api.groq.com/openai', TRUE, TRUE, '{}'::jsonb)"#,
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("seed llm configs");

    let secret =
        repo.find_default_secret(&scope, "groq").await.expect("query default secret").expect("default secret exists");
    assert_eq!(secret.encrypted_api_key, "cipher-new");
    assert_eq!(secret.base_url.as_deref(), Some("https://api.groq.com/openai"));

    let encrypted_key = repo.find_default_api_key(&scope, "groq").await.expect("query default key");
    assert_eq!(encrypted_key.as_deref(), Some("cipher-new"));
}
