use backend_flow_sdk::flow::StepRef;
use backend_flow_sdk::WebhookStep;
use std::sync::Arc;

pub fn steps() -> Vec<StepRef> {
    vec![Arc::new(WebhookStep::new())]
}
