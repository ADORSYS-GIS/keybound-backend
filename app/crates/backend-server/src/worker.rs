use crate::sms_provider::{ApiSmsProvider, ConsoleSmsProvider, SmsProvider, SnsSmsProvider};
use crate::state::AppState;
use crate::state_machine::engine::Engine;
use crate::state_machine::jobs::StateMachineStepJob;
use crate::state_machine::queue::queue_namespace as sm_queue_namespace;
use apalis::prelude::{BoxDynError, TaskSink, WorkerBuilder};
use apalis_redis::{RedisConfig, RedisStorage};
use async_trait::async_trait;
use backend_core::SmsProviderType;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

const NOTIFICATION_QUEUE_NAMESPACE: &str = "backend:notifications";

pub async fn ensure_redis_ready(redis_url: &str) -> backend_core::Result<()> {
    let client = redis::Client::open(redis_url)
        .map_err(|error| backend_core::Error::Server(format!("invalid redis url: {error}")))?;

    let mut connection = client
        .get_multiplexed_async_connection()
        .await
        .map_err(|error| {
            backend_core::Error::Server(format!(
                "failed to connect to redis at startup: {error}"
            ))
        })?;

    let response: String = connection.ping().await.map_err(|error| {
        backend_core::Error::Server(format!("failed to ping redis at startup: {error}"))
    })?;
    if response != "PONG" {
        return Err(backend_core::Error::Server(format!(
            "unexpected redis PING response at startup: {response}"
        )));
    }

    Ok(())
}

#[async_trait]
pub trait WorkerHttpClient: Send + Sync + std::fmt::Debug {
    async fn post_json(
        &self,
        url: &str,
        body: &serde_json::Value,
    ) -> Result<(http::StatusCode, String), BoxDynError>;
}

#[async_trait]
impl WorkerHttpClient for reqwest::Client {
    async fn post_json(
        &self,
        url: &str,
        body: &serde_json::Value,
    ) -> Result<(http::StatusCode, String), BoxDynError> {
        let response = self
            .post(url)
            .header("User-Agent", "user-storage/1.0.0")
            .json(body)
            .send()
            .await
            .map_err(|e| Box::new(e) as BoxDynError)?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| Box::new(e) as BoxDynError)?;

        Ok((status, text))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NotificationJob {
    Otp {
        step_id: String,
        msisdn: String,
        otp: String,
    },
    MagicEmail {
        step_id: String,
        email: String,
        token: String,
    },
}

#[async_trait]
pub trait NotificationQueue: Send + Sync {
    async fn enqueue(&self, job: NotificationJob) -> backend_core::Result<()>;
}

pub struct RedisNotificationQueue {
    redis_url: String,
}

impl RedisNotificationQueue {
    pub fn new(redis_url: String) -> Self {
        Self { redis_url }
    }
}

#[async_trait]
impl NotificationQueue for RedisNotificationQueue {
    async fn enqueue(&self, job: NotificationJob) -> backend_core::Result<()> {
        let conn = apalis_redis::connect(self.redis_url.clone())
            .await
            .map_err(|error| backend_core::Error::Server(error.to_string()))?;
        let mut storage =
            RedisStorage::new_with_config(conn, RedisConfig::new(NOTIFICATION_QUEUE_NAMESPACE));
        storage
            .push(job)
            .await
            .map_err(|error| backend_core::Error::Server(error.to_string()))?;
        Ok(())
    }
}

// Legacy type kept for compatibility; the new revamp uses state machines instead.
#[async_trait]
pub trait ProvisioningQueue: Send + Sync {
    async fn enqueue_fineract_provisioning(&self, _user_id: &str) -> backend_core::Result<()> {
        Ok(())
    }
}

pub struct RedisProvisioningQueue {
    _redis_url: String,
}

impl RedisProvisioningQueue {
    pub fn new(redis_url: String) -> Self {
        Self {
            _redis_url: redis_url,
        }
    }
}

#[async_trait]
impl ProvisioningQueue for RedisProvisioningQueue {}

pub async fn run(state: Arc<AppState>) -> backend_core::Result<()> {
    let redis_url = state.config.redis.url.clone();

    let conn = apalis_redis::connect(redis_url.clone())
        .await
        .map_err(|error| backend_core::Error::Server(error.to_string()))?;
    let sm_storage =
        RedisStorage::new_with_config(conn, RedisConfig::new(sm_queue_namespace()));

    let conn = apalis_redis::connect(redis_url.clone())
        .await
        .map_err(|error| backend_core::Error::Server(error.to_string()))?;
    let notification_storage =
        RedisStorage::new_with_config(conn, RedisConfig::new(NOTIFICATION_QUEUE_NAMESPACE));

    let sms_provider = build_sms_provider(&state.config).await?;

    let sm_state = state.clone();
    let sm_sms = sms_provider.clone();
    let sm_worker = WorkerBuilder::new("state-machine-worker")
        .backend(sm_storage)
        .build(move |job: StateMachineStepJob| {
            let state = sm_state.clone();
            let sms = sm_sms.clone();
            async move {
                let engine = Engine::new(state);
                engine
                    .process_step_job(job, sms)
                    .await
                    .map_err(|e| Box::new(e) as BoxDynError)
            }
        });

    let notification_worker = WorkerBuilder::new("notification-worker")
        .backend(notification_storage)
        .build(move |job: NotificationJob| {
            let provider = sms_provider.clone();
            async move { process_notification_job(provider, job).await }
        });

    info!("starting workers");

    tokio::select! {
        run_result = sm_worker.run() => {
            run_result.map_err(|error| backend_core::Error::Server(error.to_string()))?;
        }
        run_result = notification_worker.run() => {
            run_result.map_err(|error| backend_core::Error::Server(error.to_string()))?;
        }
        _ = tokio::signal::ctrl_c() => {
            info!("ctrl+c received, stopping workers");
        }
    }

    Ok(())
}

async fn build_sms_provider(
    cfg: &backend_core::Config,
) -> backend_core::Result<Arc<dyn SmsProvider>> {
    let provider: Arc<dyn SmsProvider> = if let Some(sms_cfg) = &cfg.sms {
        match sms_cfg.provider {
            SmsProviderType::Console => Arc::new(ConsoleSmsProvider),
            SmsProviderType::Sns => {
                let shared_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
                    .load()
                    .await;

                let mut builder = aws_sdk_sns::config::Builder::from(&shared_config);
                if let Some(sns_cfg) = &cfg.sns
                    && let Some(region) = &sns_cfg.region
                {
                    builder = builder.region(aws_types::region::Region::new(region.clone()));
                }
                let sns = aws_sdk_sns::Client::from_conf(builder.build());
                Arc::new(SnsSmsProvider::new(sns))
            }
            SmsProviderType::Api => {
                let Some(api_cfg) = sms_cfg.api.clone() else {
                    return Err(backend_core::Error::Server(
                        "sms.provider=api requires sms.api config".to_owned(),
                    ));
                };
                let client = reqwest::Client::builder()
                    .build()
                    .map_err(|e| backend_core::Error::Server(e.to_string()))?;
                Arc::new(ApiSmsProvider::new(client, api_cfg.base_url, api_cfg.auth_token))
            }
        }
    } else {
        Arc::new(ConsoleSmsProvider)
    };

    Ok(provider)
}

async fn process_notification_job(
    sms_provider: Arc<dyn SmsProvider>,
    job: NotificationJob,
) -> Result<(), BoxDynError> {
    match job {
        NotificationJob::Otp {
            step_id,
            msisdn,
            otp,
        } => {
            sms_provider.send_otp(&msisdn, &otp).await?;
            tracing::info!(step_id = %step_id, msisdn = %msisdn, "otp notification delivered");
        }
        NotificationJob::MagicEmail {
            step_id,
            email,
            token,
        } => {
            tracing::info!(
                step_id = %step_id,
                email = %email,
                token = %token,
                "magic email notification produced"
            );
        }
    }

    Ok(())
}
