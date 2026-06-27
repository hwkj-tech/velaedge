use anyhow::Result;
use cloud_api::gateway::{serve_edgelink_gateway, serve_edgelink_gateway_with_sqlite};
use cloud_api::{app, AppState};
use tokio::net::TcpListener;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    let database_url = std::env::var("EDGEOPS_CLOUD_DB")
        .unwrap_or_else(|_| "sqlite://data/cloud-agent.sqlite".to_string());
    let state = AppState::with_sqlite(&database_url).await?;
    info!(database_url = %database_url, "cloud agent sqlite store ready");

    let gateway_addr =
        std::env::var("EDGEOPS_GATEWAY_ADDR").unwrap_or_else(|_| "127.0.0.1:18080".to_string());
    let gateway_listener = TcpListener::bind(&gateway_addr).await?;
    let gateway_store = state.store.clone();
    let gateway_sqlite_store = state.sqlite_store.clone();
    tokio::spawn(async move {
        info!(addr = %gateway_addr, "EdgeLink gateway listening");
        let result = if let Some(sqlite_store) = gateway_sqlite_store {
            serve_edgelink_gateway_with_sqlite(gateway_listener, gateway_store, sqlite_store).await
        } else {
            serve_edgelink_gateway(gateway_listener, gateway_store).await
        };
        if let Err(error) = result {
            error!(error = %error, "EdgeLink gateway stopped");
        }
    });

    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    info!("cloud HTTP API listening on 127.0.0.1:8080");
    axum::serve(listener, app(state)).await?;
    Ok(())
}
