//! Identity aggregate — organization, team, and group repositories.

pub mod group;
pub mod organization;
pub mod team;

pub use group::GroupRepository;
pub use organization::OrganizationRepository;
pub use team::TeamRepository;
