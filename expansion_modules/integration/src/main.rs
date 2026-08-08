use clap::Parser;
use tracing::info;

#[derive(Parser)]
#[command(name = "expansion-server", version, about = "THE-BRIDGE Expansion Modules Server")]
struct Args {
    #[arg(short, long, default_value = "0.0.0.0:8080")]
    http_bind: String,

    #[arg(short = 'g', long, default_value = "0.0.0.0:9090")]
    grpc_bind: String,

    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(&args.log_level)
        .init();

    info!("THE-BRIDGE Expansion Modules Server");
    info!("HTTP: {}", args.http_bind);
    info!("gRPC: {}", args.grpc_bind);

    let builder = the_bridge_expansion_integration::ExpansionBuilder::new()
        .with_http(args.http_bind.parse().unwrap_or("0.0.0.0:8080".parse()?))
        .with_grpc(args.grpc_bind.parse().unwrap_or("0.0.0.0:9090".parse()?))
        .with_ai_ceo(ai_ceo_bridge::BridgeConfig::default())
        .with_auth(pwa_auth::AuthConfig::default())
        .with_fiat(fiat_offramp::FiatConfig::default())
        .with_tracing(&args.log_level);

    let server = builder.build();
    server.run().await?;

    Ok(())
}
