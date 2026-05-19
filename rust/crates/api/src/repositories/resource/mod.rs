//! Resource aggregate — member, permission, and profile repositories.

pub mod member;
pub mod permission;
pub mod profile;

pub use member::{ResourceMember, ResourceMemberRepository};
pub use permission::ResourcePermissionRepository;
pub use profile::ResourceProfileRepository;
