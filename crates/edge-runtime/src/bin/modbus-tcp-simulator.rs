use std::{net::SocketAddr, time::Duration};

use anyhow::Result;
use clap::Parser;
use edge_runtime::{DynamicFloatPoint, ModbusTcpSimulator, ModbusTcpSimulatorOptions};
use tracing::info;

#[derive(Debug, Parser)]
#[command(name = "modbus-tcp-simulator")]
#[command(about = "Runs a dynamic Modbus TCP pump simulator for edge-runtime acceptance")]
struct Args {
    #[arg(long, default_value = "127.0.0.1:1502")]
    bind: SocketAddr,
    #[arg(long, default_value_t = 1)]
    unit_id: u8,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "edge_runtime=info".into()),
        )
        .init();

    let args = Args::parse();
    let mut options = ModbusTcpSimulatorOptions::new(args.bind);
    options.unit_id = args.unit_id;
    options.dynamic_holding_floats.insert(
        10,
        DynamicFloatPoint::new(2.4, 0.18, Duration::from_secs(20)),
    );
    options.dynamic_holding_floats.insert(
        12,
        DynamicFloatPoint::new(2.6, 0.12, Duration::from_secs(16)),
    );
    options.toggling_coils.insert(0, Duration::from_secs(15));
    options.toggling_coils.insert(6, Duration::from_secs(10));
    options.input_registers.insert(0, 36);

    let simulator = ModbusTcpSimulator::bind(options).await?;
    info!(
        address = %simulator.local_addr()?,
        unit_id = args.unit_id,
        "dynamic Modbus TCP simulator started"
    );
    simulator.run().await
}
