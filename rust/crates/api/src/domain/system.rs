//! Domain contracts for process-level system endpoints and middleware.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HealthDependencyChecks {
    pub(crate) database: bool,
    pub(crate) redis: bool,
    pub(crate) nats: bool,
    pub(crate) docker: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HealthReadiness {
    checks: HealthDependencyChecks,
    ready: bool,
}

impl HealthReadiness {
    pub(crate) fn evaluate(checks: HealthDependencyChecks, nats_required: bool) -> Self {
        Self { checks, ready: checks.database && (!nats_required || checks.nats) }
    }

    pub(crate) fn is_ready(self) -> bool {
        self.ready
    }

    pub(crate) fn response(self) -> HealthReadinessResponse {
        HealthReadinessResponse {
            ok: self.ready,
            status: if self.ready { "ready" } else { "degraded" },
            checks: HealthChecksResponse {
                database: self.checks.database,
                redis: self.checks.redis,
                nats: self.checks.nats,
                docker: self.checks.docker,
            },
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct HealthResponse {
    pub(crate) ok: bool,
    pub(crate) status: &'static str,
}

pub(crate) fn health_response() -> HealthResponse {
    HealthResponse { ok: true, status: "healthy" }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct HealthReadinessResponse {
    pub(crate) ok: bool,
    pub(crate) status: &'static str,
    pub(crate) checks: HealthChecksResponse,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct HealthChecksResponse {
    pub(crate) database: bool,
    pub(crate) redis: bool,
    pub(crate) nats: bool,
    pub(crate) docker: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct SystemErrorResponse {
    pub(crate) ok: bool,
    pub(crate) error: SystemErrorBody,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct SystemErrorBody {
    pub(crate) code: &'static str,
    pub(crate) message: &'static str,
}

pub(crate) fn internal_error_response() -> SystemErrorResponse {
    SystemErrorResponse {
        ok: false,
        error: SystemErrorBody { code: "INTERNAL_ERROR", message: "Internal server error" },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_requires_database_and_configured_nats() {
        let checks = HealthDependencyChecks { database: true, redis: false, nats: false, docker: true };

        assert!(HealthReadiness::evaluate(checks, false).is_ready());
        assert!(!HealthReadiness::evaluate(checks, true).is_ready());
    }

    #[test]
    fn readiness_response_preserves_probe_details() {
        let checks = HealthDependencyChecks { database: false, redis: true, nats: false, docker: true };
        let response = HealthReadiness::evaluate(checks, false).response();

        assert!(!response.ok);
        assert_eq!(response.status, "degraded");
        assert!(!response.checks.database);
        assert!(response.checks.redis);
        assert!(!response.checks.nats);
        assert!(response.checks.docker);
    }

    #[test]
    fn system_error_response_does_not_expose_internal_details() {
        let response = internal_error_response();

        assert!(!response.ok);
        assert_eq!(response.error.code, "INTERNAL_ERROR");
        assert_eq!(response.error.message, "Internal server error");
    }
}
