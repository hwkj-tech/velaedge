use anyhow::{bail, Context, Result};
use edge_core::{
    DiscoveredPoint, DiscoveryAddressKind, DiscoveryReport, DiscoveryRequest, EdgeConfigPackage,
    PointAddress, PointMappingSuggestion, ProtocolConnection, ProtocolType, TelemetryType,
};

use crate::{
    modbus_rtu::{build_read_holding_registers_request, parse_read_holding_registers_response},
    OpcUaAdapter, SerialBus, SerialBusFactory,
};

pub async fn run_protocol_discovery_request<F>(
    package: &EdgeConfigPackage,
    request: DiscoveryRequest,
    factory: &mut F,
) -> Result<DiscoveryReport>
where
    F: SerialBusFactory,
{
    let connection = package
        .protocol_connections
        .iter()
        .find(|connection| connection.connection_id == request.protocol_connection_id)
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "discovery connection {} is not present in active config",
                request.protocol_connection_id
            )
        })?;
    match connection.protocol {
        ProtocolType::ModbusRtu => run_modbus_discovery_request(package, request, factory).await,
        ProtocolType::OpcUa => {
            let mut adapter = OpcUaAdapter::new(connection, Vec::new())?;
            adapter.discover_variables(&request).await
        }
        protocol => bail!("runtime discovery does not support {protocol:?} connections"),
    }
}

pub async fn run_modbus_discovery_request<F>(
    package: &EdgeConfigPackage,
    request: DiscoveryRequest,
    factory: &mut F,
) -> Result<DiscoveryReport>
where
    F: SerialBusFactory,
{
    let connection = package
        .protocol_connections
        .iter()
        .find(|connection| connection.connection_id == request.protocol_connection_id)
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "discovery connection {} is not present in active config",
                request.protocol_connection_id
            )
        })?;
    let bus = factory
        .open(&connection)
        .with_context(|| format!("open discovery connection {}", connection.connection_id))?;
    ModbusRtuDiscovery::new(connection, request, bus)
        .run()
        .await
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimulatedSerialDiscovery {
    job_id: String,
    protocol_connection_id: String,
}

pub struct ModbusRtuDiscovery<B> {
    connection: ProtocolConnection,
    request: DiscoveryRequest,
    bus: B,
}

impl<B> ModbusRtuDiscovery<B>
where
    B: SerialBus,
{
    pub fn new(connection: ProtocolConnection, request: DiscoveryRequest, bus: B) -> Self {
        Self {
            connection,
            request,
            bus,
        }
    }

    pub async fn run(&mut self) -> Result<DiscoveryReport> {
        self.request
            .validate()
            .map_err(anyhow::Error::msg)
            .context("invalid discovery request")?;
        if self.connection.protocol != ProtocolType::ModbusRtu {
            bail!("Modbus RTU discovery requires a ModbusRtu protocol connection");
        }
        if self.connection.connection_id != self.request.protocol_connection_id {
            bail!("discovery request targets a different protocol connection");
        }
        if self.request.address_kind != DiscoveryAddressKind::HoldingRegister {
            bail!("Modbus RTU discovery only supports holding registers");
        }

        let mut report = DiscoveryReport::new(
            self.request.job_id.clone(),
            self.request.protocol_connection_id.clone(),
        );
        for address in self.request.start_address..=self.request.end_address {
            let register = u16::try_from(address - 40001)
                .context("Modbus holding register exceeds protocol range")?;
            let request = build_read_holding_registers_request(self.request.slave_id, register, 1);
            let Ok(response) = self.bus.transact(&request).await else {
                continue;
            };
            let Ok(registers) =
                parse_read_holding_registers_response(&response, self.request.slave_id, 1)
            else {
                continue;
            };
            report.discovered_points.push(
                DiscoveredPoint::new(
                    &self.request.protocol_connection_id,
                    PointAddress::modbus_holding_register(address),
                    TelemetryType::Integer,
                )
                .with_sample_values(vec![registers[0].to_string()])
                .with_confidence(0.75),
            );
        }
        Ok(report)
    }
}

impl SimulatedSerialDiscovery {
    pub fn new(job_id: impl Into<String>, protocol_connection_id: impl Into<String>) -> Self {
        Self {
            job_id: job_id.into(),
            protocol_connection_id: protocol_connection_id.into(),
        }
    }

    pub fn run(&self) -> DiscoveryReport {
        DiscoveryReport::new(&self.job_id, &self.protocol_connection_id)
            .with_point(
                DiscoveredPoint::new(
                    &self.protocol_connection_id,
                    PointAddress::modbus_holding_register(40001),
                    TelemetryType::Float,
                )
                .with_sample_values(vec!["220.1".to_string(), "220.3".to_string()])
                .with_confidence(0.72),
            )
            .with_suggestion(
                PointMappingSuggestion::new(
                    "meter_voltage_a",
                    "meter-1",
                    "electric.voltage_a",
                    &self.protocol_connection_id,
                    PointAddress::modbus_holding_register(40001),
                    TelemetryType::Float,
                )
                .with_unit("V")
                .with_confidence(0.82)
                .with_evidence("数值范围和波动特征符合 A 相电压"),
            )
    }
}
