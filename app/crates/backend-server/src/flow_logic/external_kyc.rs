use backend_flow_sdk::flow::StepRef;
use std::sync::Arc;

pub fn steps() -> Vec<StepRef> {
    vec![Arc::new(super::webhook_http::WebhookHttpStep::new())]
}
