//! Server-side self-fix loop: PR Bridge (import + rebuild + draft PR) and Merge Executor.
//! All privileged git runs in a server-owned clean clone; /workspace is read as plain files.

pub(crate) mod import;
