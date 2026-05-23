//! Container pool service.
//!
//! Owns read-side runtime status for the container pool surface.

use std::sync::Arc;

use agentforge_platform::DockerClient;

pub(crate) use crate::domain::agent::{PoolStatusProjection, pool_status_response};

pub(crate) struct PoolService {
    docker: Option<Arc<DockerClient>>,
}

impl PoolService {
    pub(crate) fn new(docker: Option<Arc<DockerClient>>) -> Self {
        Self { docker }
    }

    pub(crate) fn status(&self) -> PoolStatusProjection {
        PoolStatusProjection::new(self.docker.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_response_reports_missing_docker_runtime() {
        let response = pool_status_response(PoolService::new(None).status());
        assert_eq!(response["ok"], true);
        assert_eq!(response["data"]["docker_available"], false);
    }
}
