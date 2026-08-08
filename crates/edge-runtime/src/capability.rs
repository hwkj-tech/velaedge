use crate::{RuntimeProtocolCatalog, RuntimeProtocolTransport};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeCapabilityConfig {
    pub serial_protocols: Vec<String>,
    pub network_protocols: Vec<String>,
    pub mqtt_uplink_enabled: bool,
    pub local_store_backend: String,
}

impl RuntimeCapabilityConfig {
    pub fn serial_mqtt_defaults() -> Self {
        let executable = RuntimeProtocolCatalog::executable();
        Self {
            serial_protocols: executable
                .iter()
                .filter(|descriptor| descriptor.transport == RuntimeProtocolTransport::Serial)
                .map(|descriptor| descriptor.capability_id.to_string())
                .collect(),
            network_protocols: executable
                .iter()
                .filter(|descriptor| {
                    matches!(
                        descriptor.transport,
                        RuntimeProtocolTransport::Tcp
                            | RuntimeProtocolTransport::Udp
                            | RuntimeProtocolTransport::TcpUdp
                    )
                })
                .map(|descriptor| descriptor.capability_id.to_string())
                .collect(),
            mqtt_uplink_enabled: true,
            local_store_backend: "rocksdb".to_string(),
        }
    }

    pub fn capabilities(&self) -> Vec<String> {
        let mut capabilities = self
            .serial_protocols
            .iter()
            .map(|protocol| format!("protocol:{protocol}"))
            .collect::<Vec<_>>();
        capabilities.extend(
            self.network_protocols
                .iter()
                .map(|protocol| format!("protocol:{protocol}")),
        );
        if self
            .serial_protocols
            .iter()
            .any(|protocol| protocol == "custom-serial-frame-dsl-v2")
        {
            capabilities.push("protocol:custom-serial-frame-dsl-v1".to_string());
        }
        if !self.serial_protocols.is_empty() {
            capabilities.push("transport:serial".to_string());
        }
        if self.network_protocols.iter().any(|protocol| {
            RuntimeProtocolCatalog::all().iter().any(|descriptor| {
                descriptor.capability_id == protocol
                    && matches!(
                        descriptor.transport,
                        RuntimeProtocolTransport::Tcp | RuntimeProtocolTransport::TcpUdp
                    )
            })
        }) {
            capabilities.push("transport:tcp".to_string());
        }
        if self.network_protocols.iter().any(|protocol| {
            RuntimeProtocolCatalog::all().iter().any(|descriptor| {
                descriptor.capability_id == protocol
                    && matches!(
                        descriptor.transport,
                        RuntimeProtocolTransport::Udp | RuntimeProtocolTransport::TcpUdp
                    )
            })
        }) {
            capabilities.push("transport:udp".to_string());
        }
        if self.mqtt_uplink_enabled {
            capabilities.push("uplink:mqtt".to_string());
        }
        capabilities.push(format!("local-store:{}", self.local_store_backend));
        capabilities
    }
}
