use anyhow::Result;
use cloud_api::{app, AppState};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    axum::serve(listener, app(AppState::default())).await?;
    Ok(())
}
