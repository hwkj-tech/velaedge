use anyhow::Result;
use clap::Parser;
use cloud_control::{EdgeNode, FleetRegistry};
use tracing::info;

#[derive(Debug, Parser)]
#[command(name = "cloud-control")]
#[command(about = "Runs the cloud control-plane MVP")]
struct Args {
    #[arg(long, default_value = "edge-dev")]
    edge_id: String,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cloud_control=info".into()),
        )
        .init();

    let args = Args::parse();
    let mut registry = FleetRegistry::default();
    registry.register(
        EdgeNode::new(&args.edge_id, "Development Edge")
            .at_site("lab")
            .with_capability("simulated-protocol"),
    );

    info!(
        edge_id = %args.edge_id,
        node_count = registry.nodes().count(),
        "cloud control plane initialized"
    );

    Ok(())
}
