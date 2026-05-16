//! Domain rules for backend bounded contexts.
//!
//! Keep pure business policies here. Application services coordinate these
//! rules with repositories, infrastructure clients, and HTTP route DTOs.

pub(crate) mod admin;
pub(crate) mod agent;
pub(crate) mod attachment;
pub(crate) mod billing;
pub(crate) mod configuration;
pub(crate) mod credential;
pub(crate) mod license;
pub(crate) mod memory;
pub(crate) mod observability;
pub(crate) mod orchestration;
pub(crate) mod prompt_library;
pub(crate) mod resource;
pub(crate) mod skill;
pub(crate) mod turn;
pub(crate) mod user;
pub(crate) mod voice;
