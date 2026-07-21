use anyhow::{bail, Context, Result};
use cloud_api::gateway::{
    serve_edgelink_gateway_with_registry, serve_edgelink_gateway_with_registry_and_sqlite,
    serve_edgelink_tls_gateway_with_registry, serve_edgelink_tls_gateway_with_registry_and_sqlite,
    EdgeGatewayTlsConfig,
};
use cloud_api::{app, AppState, BootstrapMode};
use tokio::net::TcpListener;
use tokio::signal;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    let database_url = std::env::var("EDGEOPS_CLOUD_DB")
        .unwrap_or_else(|_| "sqlite://data/cloud-agent.sqlite".to_string());
    let bootstrap_mode = BootstrapMode::from_env()?;
    let state = AppState::with_sqlite_bootstrap(&database_url, bootstrap_mode).await?;
    info!(
        database_url = %database_url,
        ?bootstrap_mode,
        api_authentication = state.api_auth.is_enabled(),
        "cloud agent sqlite store ready"
    );

    let gateway_addr =
        std::env::var("EDGEOPS_GATEWAY_ADDR").unwrap_or_else(|_| "127.0.0.1:18080".to_string());
    let gateway_listener = TcpListener::bind(&gateway_addr).await?;
    let gateway_tls = load_gateway_tls_from_env()?;
    let gateway_store = state.store.clone();
    let gateway_sqlite_store = state.sqlite_store.clone();
    let gateway_commands = state.gateway_commands.clone();
    let gateway_task = tokio::spawn(async move {
        info!(addr = %gateway_addr, tls = gateway_tls.is_some(), "EdgeLink gateway listening");
        let result = match (gateway_tls, gateway_sqlite_store) {
            (Some(tls), Some(sqlite_store)) => {
                serve_edgelink_tls_gateway_with_registry_and_sqlite(
                    gateway_listener,
                    tls,
                    gateway_store,
                    gateway_commands,
                    sqlite_store,
                )
                .await
            }
            (Some(tls), None) => {
                serve_edgelink_tls_gateway_with_registry(
                    gateway_listener,
                    tls,
                    gateway_store,
                    gateway_commands,
                )
                .await
            }
            (None, Some(sqlite_store)) => {
                serve_edgelink_gateway_with_registry_and_sqlite(
                    gateway_listener,
                    gateway_store,
                    gateway_commands,
                    sqlite_store,
                )
                .await
            }
            (None, None) => {
                serve_edgelink_gateway_with_registry(
                    gateway_listener,
                    gateway_store,
                    gateway_commands,
                )
                .await
            }
        };
        if let Err(error) = result {
            error!(error = %error, "EdgeLink gateway stopped");
        }
    });

    let http_addr =
        std::env::var("EDGEOPS_HTTP_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let listener = TcpListener::bind(&http_addr).await?;
    info!(addr = %http_addr, "cloud HTTP API listening");
    axum::serve(listener, app(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    gateway_task.abort();
    let _ = gateway_task.await;
    info!("cloud agent stopped");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    info!("shutdown signal received");
}

fn load_gateway_tls_from_env() -> Result<Option<EdgeGatewayTlsConfig>> {
    let cert_path = std::env::var("EDGEOPS_GATEWAY_TLS_CERT").ok();
    let key_path = std::env::var("EDGEOPS_GATEWAY_TLS_KEY").ok();
    let client_ca_path = std::env::var("EDGEOPS_GATEWAY_TLS_CLIENT_CA").ok();
    match (cert_path, key_path, client_ca_path) {
        (None, None, None) => Ok(None),
        (Some(cert_path), Some(key_path), Some(client_ca_path)) => {
            let cert = std::fs::read_to_string(&cert_path)
                .with_context(|| format!("failed to read gateway TLS certificate {cert_path}"))?;
            let key = std::fs::read_to_string(&key_path)
                .with_context(|| format!("failed to read gateway TLS key {key_path}"))?;
            let client_ca = std::fs::read_to_string(&client_ca_path).with_context(|| {
                format!("failed to read gateway TLS client CA {client_ca_path}")
            })?;
            Ok(Some(EdgeGatewayTlsConfig::from_pem(
                &cert, &key, &client_ca,
            )?))
        }
        _ => bail!(
            "EDGEOPS_GATEWAY_TLS_CERT, EDGEOPS_GATEWAY_TLS_KEY and EDGEOPS_GATEWAY_TLS_CLIENT_CA must be configured together"
        ),
    }
}
