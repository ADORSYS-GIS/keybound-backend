mod branding;

use backend_cli::{AppCli, AppCommands, Parser};
use branding::banner::BANNER;
use mimalloc::MiMalloc;
use tracing::{info};
use backend_core::{load_from_path, Result};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> Result<()> {
    match AppCli::parse().command {
        Some(AppCommands::Serve { config_path }) => {
            info!("{}", BANNER);
            info!("Starting the server");

            let _ = load_from_path(&config_path)?;


        }
        Some(AppCommands::Migrate { config_path }) => {
            let config = load_from_path(&config_path)?;
            backend_migrate::migrate(&config.database.url).await?;
        }
        Some(AppCommands::Config { config_path }) => {
            let _ = load_from_path(&config_path)?;
        }
        None => {
            info!("No command provided. Use --help for more information.");
        }
    }

    Ok(())
}
