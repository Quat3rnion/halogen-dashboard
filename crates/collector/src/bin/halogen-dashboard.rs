use clap::{Parser, Subcommand};
use std::{net::SocketAddr, path::PathBuf};

#[derive(Parser)]
#[command(version, about = "The sole Collector for Halogen metrics")]
struct Args {
    #[arg(
        long,
        env = "HALOGEN_URL",
        default_value = "http://192.168.50.177:8731"
    )]
    halogen_url: String,
    /// Halogen API key, sent as a Bearer token. Needed when Halogen requires a key.
    #[arg(long, env = "HALOGEN_API_KEY", hide_env_values = true)]
    halogen_api_key: Option<String>,
    #[arg(long, env = "HALOGEN_DASHBOARD_LISTEN", default_value = "0.0.0.0:8790")]
    listen: SocketAddr,
    #[arg(
        long,
        env = "HALOGEN_DASHBOARD_DB",
        default_value = "./data/halogen-dashboard.sqlite",
        global = true
    )]
    db: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    Seed {
        #[arg(long, default_value_t = 30)]
        days: u32,
        #[arg(long, default_value_t = 42)]
        seed: u64,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let args = Args::parse();
    if let Some(Command::Seed { days, seed }) = args.command {
        let start = std::time::Instant::now();
        halogen_dashboard::seed::seed(&args.db, days, seed)?;
        println!(
            "Seeded {days} days in {:.2}s at {}",
            start.elapsed().as_secs_f64(),
            args.db.display()
        );
        return Ok(());
    }
    let app = halogen_dashboard::collector::App::new(&args.db)?;
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    tracing::info!(listen=%args.listen,halogen_url=%args.halogen_url,"Collector started");
    halogen_dashboard::collector::serve(listener, app, &args.halogen_url, args.halogen_api_key.as_deref()).await
}
