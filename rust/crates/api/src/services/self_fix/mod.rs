//! Server-side self-fix loop: PR Bridge (import + rebuild + draft PR) and Merge Executor.
//! All privileged git runs in a server-owned clean clone; /workspace is read as plain files.

// `pub(crate)` in production; widened to `pub` under `test-support` so the
// `crate::testing::self_fix_rebuild` re-export can reach these from integration
// tests. The items themselves stay `pub` either way, so production callers see
// no change in reachability (the module gate is what scopes them).
#[cfg(not(any(test, feature = "test-support")))]
pub(crate) mod import;
#[cfg(any(test, feature = "test-support"))]
pub mod import;

#[cfg(not(any(test, feature = "test-support")))]
pub(crate) mod rebuild;
#[cfg(any(test, feature = "test-support"))]
pub mod rebuild;
