use backend_flow_sdk::{flow::StepRef, WebhookStep};
use std::sync::Arc;

pub fn steps() -> Vec<StepRef> {
    vec![Arc::new(WebhookStep::new())]
}
