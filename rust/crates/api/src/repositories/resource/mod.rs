//! Resource aggregate — member, permission, and profile repositories.

pub mod invite;
#[cfg(test)]
pub mod invite_tests;
pub mod member;
pub mod navigation;
pub mod permission;
pub mod profile;

pub use invite::TeamInviteRepository;
pub use member::{ResourceMember, ResourceMemberRepository};
pub use navigation::LegacyNavigationRepository;
pub use permission::ResourcePermissionRepository;
pub use profile::ResourceProfileRepository;
