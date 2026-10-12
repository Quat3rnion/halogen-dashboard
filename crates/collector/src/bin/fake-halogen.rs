use clap::Parser;

#[derive(Parser)]
#[command(about = "Simulate Halogen 0.18.1 status surfaces")]
struct Args {
    #[arg(long, env = "FAKE_HALOGEN_LISTEN", default_value = "127.0.0.1:8791")]
    listen: std::net::SocketAddr,
    #[arg(long, env = "FAKE_HALOGEN_SEED", default_value_t = 1)]
    seed: u64,
    #[arg(long, env = "FAKE_HALOGEN_SPEED", default_value_t = 1.0)]
    speed: f64,
    /// Inject Outages, Restarts and Degraded periods. Accepts true/false from env.
    #[arg(
        long,
        env = "FAKE_HALOGEN_CHAOS",
        action = clap::ArgAction::Set,
        num_args = 0..=1,
        default_value_t = false,
        default_missing_value = "true",
        value_parser = clap::builder::BoolishValueParser::new()
    )]
    chaos: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    anyhow::ensure!(
        args.speed.is_finite() && args.speed > 0.0,
        "speed must be finite and positive"
    );
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    halogen_dashboard::simulator::serve(listener, args.seed, args.speed, args.chaos).await
}
