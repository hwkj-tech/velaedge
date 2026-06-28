use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use edge_core::SerialConnectionSettings;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::{DataBits, Parity, SerialPortBuilderExt, SerialStream, StopBits};

#[async_trait]
pub trait SerialBus: Send {
    async fn transact(&mut self, request: &[u8]) -> Result<Vec<u8>>;
}

#[derive(Clone, Debug)]
pub struct ScriptedSerialBus {
    inner: Arc<Mutex<ScriptedSerialBusState>>,
}

#[derive(Debug)]
struct ScriptedSerialBusState {
    responses: Vec<Vec<u8>>,
    requests: Vec<Vec<u8>>,
}

impl ScriptedSerialBus {
    pub fn new(responses: Vec<Vec<u8>>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ScriptedSerialBusState {
                responses: responses.into_iter().rev().collect(),
                requests: Vec::new(),
            })),
        }
    }

    pub fn requests(&self) -> Vec<Vec<u8>> {
        self.inner
            .lock()
            .expect("scripted serial bus lock should not be poisoned")
            .requests
            .clone()
    }
}

#[async_trait]
impl SerialBus for ScriptedSerialBus {
    async fn transact(&mut self, request: &[u8]) -> Result<Vec<u8>> {
        let mut state = self
            .inner
            .lock()
            .expect("scripted serial bus lock should not be poisoned");
        state.requests.push(request.to_vec());
        state
            .responses
            .pop()
            .ok_or_else(|| anyhow::anyhow!("scripted serial bus has no response left"))
    }
}

pub struct TokioSerialBus {
    port: SerialStream,
    read_idle_timeout: Duration,
    max_response_bytes: usize,
}

impl TokioSerialBus {
    pub fn open(settings: &SerialConnectionSettings) -> Result<Self> {
        let mut builder = tokio_serial::new(&settings.port, settings.baud_rate)
            .data_bits(data_bits(settings.data_bits)?)
            .stop_bits(stop_bits(settings.stop_bits)?)
            .parity(parity(&settings.parity)?);
        builder = builder.timeout(Duration::from_millis(200));
        let port = builder
            .open_native_async()
            .with_context(|| format!("failed to open serial port {}", settings.port))?;

        Ok(Self {
            port,
            read_idle_timeout: Duration::from_millis(80),
            max_response_bytes: 256,
        })
    }

    pub fn with_read_idle_timeout(mut self, timeout: Duration) -> Self {
        self.read_idle_timeout = timeout;
        self
    }

    pub fn with_max_response_bytes(mut self, max_response_bytes: usize) -> Self {
        self.max_response_bytes = max_response_bytes;
        self
    }
}

#[async_trait]
impl SerialBus for TokioSerialBus {
    async fn transact(&mut self, request: &[u8]) -> Result<Vec<u8>> {
        self.port
            .write_all(request)
            .await
            .context("failed to write serial request")?;
        self.port
            .flush()
            .await
            .context("failed to flush serial request")?;

        let mut response = Vec::new();
        let mut buffer = [0_u8; 64];
        loop {
            let read =
                tokio::time::timeout(self.read_idle_timeout, self.port.read(&mut buffer)).await;
            match read {
                Ok(Ok(0)) => break,
                Ok(Ok(count)) => {
                    response.extend_from_slice(&buffer[..count]);
                    if response.len() > self.max_response_bytes {
                        bail!("serial response exceeded maximum frame size");
                    }
                }
                Ok(Err(error)) => return Err(error).context("failed to read serial response"),
                Err(_) if response.is_empty() => bail!("serial response timed out"),
                Err(_) => break,
            }
        }

        Ok(response)
    }
}

pub fn require_serial_endpoint(port: Option<&str>) -> Result<&str> {
    let Some(port) = port else {
        bail!("serial protocol connection requires a port");
    };
    if port.trim().is_empty() {
        bail!("serial protocol connection port cannot be empty");
    }
    Ok(port)
}

fn data_bits(value: u8) -> Result<DataBits> {
    match value {
        5 => Ok(DataBits::Five),
        6 => Ok(DataBits::Six),
        7 => Ok(DataBits::Seven),
        8 => Ok(DataBits::Eight),
        _ => bail!("unsupported serial data bits: {value}"),
    }
}

fn stop_bits(value: u8) -> Result<StopBits> {
    match value {
        1 => Ok(StopBits::One),
        2 => Ok(StopBits::Two),
        _ => bail!("unsupported serial stop bits: {value}"),
    }
}

fn parity(value: &str) -> Result<Parity> {
    match value.to_ascii_lowercase().as_str() {
        "none" | "n" => Ok(Parity::None),
        "even" | "e" => Ok(Parity::Even),
        "odd" | "o" => Ok(Parity::Odd),
        _ => bail!("unsupported serial parity: {value}"),
    }
}
