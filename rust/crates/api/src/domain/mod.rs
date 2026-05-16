//! Domain rules for backend bounded contexts.
//!
//! Keep pure business policies here. Application services coordinate these
//! rules with repositories, infrastructure clients, and HTTP route DTOs.

pub(crate) mod agent;
pub(crate) mod credential;
pub(crate) mod orchestration;
