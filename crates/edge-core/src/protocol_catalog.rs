use crate::ProtocolType;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeProtocolTransport {
    Internal,
    Serial,
    Tcp,
    Udp,
    TcpUdp,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeProtocolMaturity {
    Laboratory,
    DeploymentCandidate,
    Production,
    Planned,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeProtocolDescriptor {
    pub protocol_type: ProtocolType,
    pub capability_id: &'static str,
    pub display_name: &'static str,
    pub transport: RuntimeProtocolTransport,
    pub maturity: RuntimeProtocolMaturity,
    pub telemetry_read: bool,
    pub command_write: bool,
    pub automatic_discovery: bool,
}

impl RuntimeProtocolDescriptor {
    pub fn is_executable(self) -> bool {
        self.maturity != RuntimeProtocolMaturity::Planned && self.telemetry_read
    }
}

pub struct RuntimeProtocolCatalog;

impl RuntimeProtocolCatalog {
    pub fn all() -> Vec<RuntimeProtocolDescriptor> {
        vec![
            descriptor(
                ProtocolType::Simulated,
                "simulated",
                "模拟协议",
                RuntimeProtocolTransport::Internal,
                RuntimeProtocolMaturity::Laboratory,
                true,
                true,
                false,
            ),
            descriptor(
                ProtocolType::ModbusTcp,
                "modbus-tcp",
                "Modbus TCP",
                RuntimeProtocolTransport::Tcp,
                RuntimeProtocolMaturity::DeploymentCandidate,
                true,
                true,
                false,
            ),
            descriptor(
                ProtocolType::ModbusRtu,
                "modbus-rtu",
                "Modbus RTU",
                RuntimeProtocolTransport::Serial,
                RuntimeProtocolMaturity::DeploymentCandidate,
                true,
                true,
                true,
            ),
            descriptor(
                ProtocolType::Dlt645,
                "dlt645-2007",
                "DL/T 645-2007",
                RuntimeProtocolTransport::Serial,
                RuntimeProtocolMaturity::DeploymentCandidate,
                true,
                false,
                false,
            ),
            descriptor(
                ProtocolType::Iec101,
                "iec60870-5-101-unbalanced",
                "IEC 60870-5-101",
                RuntimeProtocolTransport::Serial,
                RuntimeProtocolMaturity::DeploymentCandidate,
                true,
                true,
                false,
            ),
            descriptor(
                ProtocolType::Iec104,
                "iec60870-5-104-client",
                "IEC 60870-5-104",
                RuntimeProtocolTransport::Tcp,
                RuntimeProtocolMaturity::DeploymentCandidate,
                true,
                true,
                false,
            ),
            descriptor(
                ProtocolType::CustomSerial,
                "custom-serial-frame-dsl-v2",
                "自定义串口帧 DSL",
                RuntimeProtocolTransport::Serial,
                RuntimeProtocolMaturity::DeploymentCandidate,
                true,
                false,
                false,
            ),
            descriptor(
                ProtocolType::OpcUa,
                "opc-ua-client",
                "OPC UA",
                RuntimeProtocolTransport::Tcp,
                RuntimeProtocolMaturity::DeploymentCandidate,
                true,
                true,
                true,
            ),
            descriptor(
                ProtocolType::BacnetIp,
                "bacnet-ip",
                "BACnet/IP",
                RuntimeProtocolTransport::Udp,
                RuntimeProtocolMaturity::DeploymentCandidate,
                true,
                true,
                false,
            ),
            descriptor(
                ProtocolType::SiemensS7,
                "siemens-s7",
                "Siemens S7",
                RuntimeProtocolTransport::Tcp,
                RuntimeProtocolMaturity::DeploymentCandidate,
                true,
                true,
                false,
            ),
            descriptor(
                ProtocolType::OmronFins,
                "omron-fins",
                "Omron FINS",
                RuntimeProtocolTransport::TcpUdp,
                RuntimeProtocolMaturity::DeploymentCandidate,
                true,
                true,
                false,
            ),
        ]
    }

    pub fn descriptor(protocol: ProtocolType) -> RuntimeProtocolDescriptor {
        Self::all()
            .into_iter()
            .find(|descriptor| descriptor.protocol_type == protocol)
            .expect("every ProtocolType must have a runtime protocol descriptor")
    }

    pub fn executable() -> Vec<RuntimeProtocolDescriptor> {
        Self::all()
            .into_iter()
            .filter(|descriptor| descriptor.is_executable())
            .collect()
    }
}

#[allow(clippy::too_many_arguments)]
fn descriptor(
    protocol_type: ProtocolType,
    capability_id: &'static str,
    display_name: &'static str,
    transport: RuntimeProtocolTransport,
    maturity: RuntimeProtocolMaturity,
    telemetry_read: bool,
    command_write: bool,
    automatic_discovery: bool,
) -> RuntimeProtocolDescriptor {
    RuntimeProtocolDescriptor {
        protocol_type,
        capability_id,
        display_name,
        transport,
        maturity,
        telemetry_read,
        command_write,
        automatic_discovery,
    }
}
