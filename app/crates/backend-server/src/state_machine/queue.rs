use crate::state_machine::jobs::StateMachineStepJob;
use apalis::prelude::TaskSink;
use apalis_redis::{RedisConfig, RedisStorage};
use async_trait::async_trait;

const SM_STEP_QUEUE_NAMESPACE: &str = "backend:sm_steps";

#[async_trait]
pub trait StateMachineQueue: Send + Sync {
    async fn enqueue(&self, job: StateMachineStepJob) -> backend_core::Result<()>;
}

pub struct RedisStateMachineQueue {
    redis_url: String,
}

impl RedisStateMachineQueue {
    pub fn new(redis_url: String) -> Self {
        Self { redis_url }
    }
}

#[async_trait]
impl StateMachineQueue for RedisStateMachineQueue {
    async fn enqueue(&self, job: StateMachineStepJob) -> backend_core::Result<()> {
        let conn = apalis_redis::connect(self.redis_url.clone())
            .await
            .map_err(|error| backend_core::Error::Server(error.to_string()))?;
        let mut storage =
            RedisStorage::new_with_config(conn, RedisConfig::new(SM_STEP_QUEUE_NAMESPACE));
        storage
            .push(job)
            .await
            .map_err(|error| backend_core::Error::Server(error.to_string()))?;
        Ok(())
    }
}

pub fn queue_namespace() -> &'static str {
    SM_STEP_QUEUE_NAMESPACE
}
