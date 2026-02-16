use backend_core::Config;
use backend_repository::{
    ApprovalRepository, DeviceRepository, KycRepository, SmsRepository, UserRepository,
};
use sqlx::postgres::PgPoolOptions;
use tracing::info;

#[derive(Clone)]
pub struct AppState {
    pub kyc: KycRepository,
    pub user: UserRepository,
    pub device: DeviceRepository,
    pub approval: ApprovalRepository,
    pub sms: SmsRepository,
    pub s3: aws_sdk_s3::Client,
    pub sns: aws_sdk_sns::Client,
    pub config: Config,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("kyc", &"<KycRepository>")
            .field("user", &"<UserRepository>")
            .field("device", &"<DeviceRepository>")
            .field("approval", &"<ApprovalRepository>")
            .field("sms", &"<SmsRepository>")
            .field("s3", &"<S3Client>")
            .field("sns", &"<SnsClient>")
            .field("config", &self.config)
            .field("http_cache", &"<HttpCache>")
            .finish()
    }
}

impl AppState {
    pub async fn from_config(cfg: &Config) -> backend_core::Result<Self> {
        info!("initializing application state and repositories");
        let db = PgPoolOptions::new()
            .max_connections(cfg.database_pool_size())
            .connect(&cfg.database.url)
            .await?;

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

        let sns = {
            let mut builder = aws_sdk_sns::config::Builder::from(&shared_config);
            if let Some(sns_cfg) = &cfg.sns {
                if let Some(region) = &sns_cfg.region {
                    builder = builder.region(aws_types::region::Region::new(region.clone()));
                }
            }
            aws_sdk_sns::Client::from_conf(builder.build())
        };

        let kyc = KycRepository::new(db.clone());
        let user = UserRepository::new(db.clone());
        let device = DeviceRepository::new(db.clone());
        let approval = ApprovalRepository::new(db.clone());
        let sms = SmsRepository::new(db);

        Ok(Self {
            kyc,
            user,
            device,
            approval,
            sms,
            s3,
            sns,
            config: cfg.clone(),
        })
    }
}
