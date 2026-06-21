//! Self-fix PR-bridge job protocol shared between the producer (`complete_task`)
//! and the consumer (`SelfFixPrWorker`). The queue name is the `job_queue.queue`
//! discriminator (mirrors `clone_protocol::CLONE_JOB_QUEUE`).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// `job_queue.queue` value the self-fix PR worker dequeues.
pub const SELF_FIX_PR_QUEUE: &str = "self_fix_pr";

/// Payload of a self-fix PR-bridge job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfFixPrJob {
    pub task_id: Uuid,
    pub org_id: Uuid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_fix_pr_job_roundtrips() {
        let job = SelfFixPrJob { task_id: Uuid::nil(), org_id: Uuid::nil() };
        let json = serde_json::to_value(&job).unwrap();
        let back: SelfFixPrJob = serde_json::from_value(json).unwrap();
        assert_eq!(back.task_id, job.task_id);
        assert_eq!(back.org_id, job.org_id);
        assert_eq!(SELF_FIX_PR_QUEUE, "self_fix_pr");
    }
}
