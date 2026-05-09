//! Small cross-cutting helpers that don't belong to a specific route or
//! service. Each submodule must stay stateless and small; anything that
//! grows to carry its own tests + mutable state should become its own
//! service module instead.

pub mod slug;
