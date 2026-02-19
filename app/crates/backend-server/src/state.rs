use crate::sms_provider::SmsProvider;
use backend_auth::{HttpClient, OidcState, SignatureState};
use backend_core::Config;
use backend_repository::{DeviceRepo, DeviceRepository, KycRepo, KycRepository, UserRepo, UserRepository};
use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::deadpool::Pool;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

#[derive(Clone)]
pub struct AppState {
    pub kyc: Arc<dyn KycRepo>,
    pub user: Arc<dyn UserRepo>,
    pub device: Arc<dyn DeviceRepo>,
    pub sms: Arc<dyn SmsProvider>,
    pub s3: aws_sdk_s3::Client,
    pub config: Config,
    pub oidc_state: Arc<OidcState>,
    pub signature_state: Arc<SignatureState>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("kyc", &"<KycRepository>")
            .field("user", &"<UserRepository>")
            .field("device", &"<DeviceRepository>")
            .field("s3", &"<S3Client>")
            .field("config", &self.config)
            .field("oidc_state", &"<OidcState>")
            .field("signature_state", &"<SignatureState>")
            .finish()
    }
}

impl AppState {
    pub async fn from_config(
        cfg: &Config,
        pool: Pool<AsyncPgConnection>,
    ) -> backend_core::Result<Self> {
        info!("initializing application state and repositories");

        let shared_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .load()
            .await;

        let s3 = {
            let mut builder = aws_sdk_s3::config::Builder::from(&shared_config);
            if let Some(s3_cfg) = &cfg.s3 {
                if let Some(region) = &s3_cfg.region {
                    builder = builder.region(aws_types::region::Region::new(region.clone()));
                }
                if let Some(endpoint) = &s3_cfg.endpoint {
                    builder = builder.endpoint_url(endpoint);
                }
                if s3_cfg.force_path_style.unwrap_or(false) {
                    builder = builder.force_path_style(true);
                }
            }
            aws_sdk_s3::Client::from_conf(builder.build())
        };

        let kyc: Arc<dyn KycRepo> = Arc::new(KycRepository::new(pool.clone()));
        let user: Arc<dyn UserRepo> = Arc::new(UserRepository::new(pool.clone()));
        let device: Arc<dyn DeviceRepo> = Arc::new(DeviceRepository::new(pool.clone()));

        let sms: Arc<dyn SmsProvider> = match cfg.sms.as_ref().map(|s| &s.provider) {
            Some(backend_core::SmsProviderType::Sns) => {
                let sns_client = aws_sdk_sns::Client::new(&shared_config);
                Arc::new(crate::sms_provider::SnsSmsProvider::new(sns_client))
            }
            _ => Arc::new(crate::sms_provider::ConsoleSmsProvider),
        };

        let http_client = HttpClient::new_with_defaults()?;

        let oidc_state = Arc::new(OidcState::new(
            cfg.oauth2.issuer.clone(),
            None, // TODO: add audiences to config if needed
            Duration::from_secs(3600),
            Duration::from_secs(3600),
            http_client,
        ));

        let signature_state = Arc::new(SignatureState {
            signature_secret: cfg.kc.signature_secret.clone(),
            max_clock_skew_seconds: cfg.kc.max_clock_skew_seconds,
            max_body_bytes: cfg.kc.max_body_bytes,
        });

        Ok(Self {
            kyc,
            user,
            device,
            sms,
            s3,
            config: cfg.clone(),
            oidc_state,
            signature_state,
        })
    }
}
