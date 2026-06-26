//! Model-aware vision (image-input) capability.
//!
//! The provider-level `capability_profile().supports_image_input` is too coarse:
//! e.g. the OpenAI provider reports `true` for *every* first-party model, so a
//! text-only model (`gpt-3.5-turbo`) on a vision-capable provider would pass the
//! image gate and only be rejected upstream — after the user already uploaded.
//! This module narrows the gate to the specific `(provider, model)` pair.
//!
//! Policy: allowlist by vision-capable model family, conservative `false` for
//! unknowns. A wrongly-excluded model fails closed with a clear message (the
//! user re-picks a model) rather than wasting an upload on an upstream rejection.

/// OpenAI model-name prefixes whose families accept image input. Allowlist, not
/// denylist: base `gpt-4`/`gpt-4-0613`, `gpt-3.5*`, and `text-*` are NOT vision
/// and correctly fall through to `false`.
///
/// Update this list (the single source of truth) when a new vision family ships.
const OPENAI_VISION_PREFIXES: &[&str] = &[
    "gpt-4o",      // gpt-4o, gpt-4o-mini, gpt-4o-2024-*, realtime, etc.
    "gpt-4-turbo", // gpt-4-turbo, gpt-4-turbo-2024-*
    "gpt-4.1",     // gpt-4.1, gpt-4.1-mini, gpt-4.1-nano
    "gpt-5",       // gpt-5, gpt-5.x, gpt-5-2025-*
];

/// Whether `model` on `provider` accepts image input.
///
/// The Anthropic and Google (Gemini) catalogs are currently all-vision, so they
/// return `true` for any model (matching their hardcoded `capability_profile`).
/// NOTE the provider KEY for Gemini is `"google"` (the registry key and
/// `GeminiProvider::name()`), not `"gemini"` — gating on the wrong key would
/// reject every Google image prompt. `openai` is gated to the vision families
/// above. Every other provider (ollama, groq, deepseek, the CN coding providers,
/// openai-compatible, custom base_url, …) is conservatively `false`, matching the
/// providers that do not advertise first-party vision.
pub fn model_supports_image(provider: &str, model: &str) -> bool {
    let provider = provider.trim().to_ascii_lowercase();
    let model = model.trim().to_ascii_lowercase();
    match provider.as_str() {
        "anthropic" => true,
        // "google" is canonical; "gemini" accepted defensively for any drift.
        "google" | "gemini" => true,
        "openai" => OPENAI_VISION_PREFIXES.iter().any(|prefix| model.starts_with(prefix)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::model_supports_image;

    #[test]
    fn anthropic_and_google_are_vision_for_any_model() {
        assert!(model_supports_image("anthropic", "claude-sonnet-4-6"));
        assert!(model_supports_image("anthropic", "claude-opus-4-8"));
        // Gemini agents are stored under the canonical provider key "google".
        assert!(model_supports_image("google", "gemini-2.5-pro"));
        assert!(model_supports_image("google", "gemini-2.0-flash"));
        assert!(model_supports_image("gemini", "gemini-2.5-pro")); // defensive alias
        assert!(model_supports_image("Anthropic", "Claude-3-Haiku")); // case-insensitive
    }

    #[test]
    fn openai_vision_families_pass() {
        for model in [
            "gpt-4o",
            "gpt-4o-mini",
            "gpt-4o-2024-11-20",
            "gpt-4-turbo",
            "gpt-4-turbo-2024-04-09",
            "gpt-4.1",
            "gpt-4.1-mini",
            "gpt-5",
            "gpt-5.4",
            "gpt-5-2025-08-01",
            "GPT-4O", // case-insensitive
        ] {
            assert!(model_supports_image("openai", model), "{model} should be vision-capable");
        }
    }

    #[test]
    fn openai_text_only_and_unknown_models_fail() {
        for model in ["gpt-3.5-turbo", "gpt-4", "gpt-4-0613", "text-davinci-003", "o1-mini", "my-custom-model", ""] {
            assert!(!model_supports_image("openai", model), "{model} should NOT pass the vision gate");
        }
    }

    #[test]
    fn non_first_party_providers_are_conservatively_false() {
        for provider in ["ollama", "groq", "deepseek", "zhipu_coding", "openai_compatible", "custom"] {
            assert!(!model_supports_image(provider, "gpt-4o"), "{provider} must default to no-vision");
            assert!(!model_supports_image(provider, "llava"), "{provider} must default to no-vision");
        }
    }
}
