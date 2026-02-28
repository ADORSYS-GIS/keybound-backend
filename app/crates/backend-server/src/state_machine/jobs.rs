use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMachineStepJob {
    pub instance_id: String,
    pub step_name: String,
    pub attempt_id: String,
}

