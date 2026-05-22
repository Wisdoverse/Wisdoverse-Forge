//! Application services for process-level system endpoints.

use crate::domain::system::{HealthDependencyChecks, HealthReadiness};

pub(crate) struct HealthReadinessService;

impl HealthReadinessService {
    pub(crate) fn evaluate(checks: HealthDependencyChecks, nats_required: bool) -> HealthReadiness {
        HealthReadiness::evaluate(checks, nats_required)
    }
}
