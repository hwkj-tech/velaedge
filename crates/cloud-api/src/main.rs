use anyhow::Result;
use cloud_api::gateway::serve_edgelink_gateway;
use cloud_api::{app, AppState};
use tokio::net::TcpListener;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    let state = AppState::default();

    let gateway_addr =
        std::env::var("EDGEOPS_GATEWAY_ADDR").unwrap_or_else(|_| "127.0.0.1:18080".to_string());
    let gateway_listener = TcpListener::bind(&gateway_addr).await?;
    let gateway_store = state.store.clone();
    tokio::spawn(async move {
        info!(addr = %gateway_addr, "EdgeLink gateway listening");
        if let Err(error) = serve_edgelink_gateway(gateway_listener, gateway_store).await {
            error!(error = %error, "EdgeLink gateway stopped");
        }
    });

    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    info!("cloud HTTP API listening on 127.0.0.1:8080");
    axum::serve(listener, app(state)).await?;
    Ok(())
}
