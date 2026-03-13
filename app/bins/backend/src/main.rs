mod branding;

#[allow(unused_imports)]
use openssl_sys as _;

use backend_core::{Cli, Commands, Result, RuntimeMode, init_tracing, load_from_path};
use backend_server::{run_worker, serve};
use branding::banner::BANNER;
use clap::Parser;
use mimalloc::MiMalloc;
use tracing::info;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> Result<()> {
    print!("{}", BANNER);

    match Cli::parse().command {
        Some(Commands::Serve { config_path, mode }) => {
            let mut config = load_from_path(&config_path)?;
            config.runtime.mode = mode.into();
            init_tracing(&config.logging);

            match config.runtime.mode {
                RuntimeMode::Server => {
                    info!("starting in server mode");
                    serve(&config).await?;
                }
                RuntimeMode::Worker => {
                    info!("starting in worker mode");
                    run_worker(&config).await?;
                }
                RuntimeMode::Shared => {
                    info!("starting in shared mode");
                    tokio::try_join!(serve(&config), run_worker(&config))?;
                }
            }
        }
        Some(Commands::Migrate { config_path }) => {
            let config = load_from_path(&config_path)?;
            init_tracing(&config.logging);

            backend_migrate::connect_postgres_and_migrate(&config.database.url).await?;
        }
        Some(Commands::Config { config_path }) => {
            let _ = load_from_path(&config_path)?;
        }
        None => {
            info!("No command provided. Use --help for more information.");
        }
    }

    Ok(())
}
