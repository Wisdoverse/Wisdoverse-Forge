//! Domain rules for backend bounded contexts.
//!
//! Keep pure business policies here. Application services coordinate these
//! rules with repositories, infrastructure clients, and HTTP route DTOs.

pub(crate) mod admin;
pub(crate) mod agent;
pub(crate) mod configuration;
pub(crate) mod credential;
pub(crate) mod observability;
pub(crate) mod orchestration;
pub(crate) mod resource;
pub(crate) mod user;
pub(crate) mod voice;
