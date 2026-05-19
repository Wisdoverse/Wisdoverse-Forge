//! Credential aggregate — API key, SSH key, git, and CLI credential repositories.

pub mod api_key;
pub mod cli;
pub mod git;
pub mod ssh_key;

pub use api_key::ApiKeyRepository;
pub use cli::{CliCredentialRepository, CliCredentialStatus, EncryptedWithRevocation};
pub use git::GitCredentialRepository;
pub use ssh_key::SshKeyRepository;
