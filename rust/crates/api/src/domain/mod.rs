//! Domain rules for backend bounded contexts.
//!
//! Keep pure business policies here. Application services coordinate these
//! rules with repositories, infrastructure clients, and HTTP route DTOs.

pub(crate) mod admin;
pub mod agent;
pub(crate) mod agent_workspace;
pub(crate) mod attachment;
pub(crate) mod auth;
pub mod auth_callout;
pub(crate) mod billing;
pub(crate) mod cli_auth_proxy;
pub(crate) mod cli_image;
pub(crate) mod configuration;
pub(crate) mod context;
pub(crate) mod context_envelope;
pub(crate) mod context_governance;
pub(crate) mod context_preview;
pub mod context_resolver;
pub(crate) mod credential;
pub(crate) mod dev_environment;
pub(crate) mod email;
pub(crate) mod evidence_projection;
pub(crate) mod gateway;
pub(crate) mod inbox;
pub(crate) mod license;
pub(crate) mod mcp;
pub(crate) mod memory;
pub(crate) mod navigation;
pub(crate) mod observability;
pub(crate) mod orchestration;
pub(crate) mod project_clone;
pub(crate) mod prompt;
pub(crate) mod prompt_library;
pub(crate) mod resource;
pub(crate) mod runtime_capability;
pub(crate) mod skill;
pub(crate) mod system;
pub(crate) mod task_context;
pub(crate) mod turn;
pub(crate) mod usage_analytics;
pub(crate) mod user;
pub(crate) mod voice;
