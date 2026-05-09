-- rust/crates/db/migrations/032_agents_system_prompt.sql
ALTER TABLE agents ADD COLUMN system_prompt TEXT;

-- Normalize provider case so frontend title-case rows match existing
-- lowercase provider keys already used by `user_llm_configs.provider`
-- and LlmProvider::name(): "anthropic" / "openai" / "google" / "ollama".
--
-- NOTE: uses "google" (not "gemini") to align with the existing
-- user_llm_configs rows and the OpenAI/Anthropic/Google/Ollama set that
-- ships in this codebase. The new GeminiProvider MUST register under
-- key "google" (see Task 8 + Task 16).
UPDATE agents SET provider = lower(provider) WHERE provider IS NOT NULL;
