#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeCapabilityConfig {
    pub serial_protocols: Vec<String>,
    pub mqtt_uplink_enabled: bool,
    pub local_store_backend: String,
}

impl RuntimeCapabilityConfig {
    pub fn serial_mqtt_defaults() -> Self {
        Self {
            serial_protocols: vec!["modbus-rtu".to_string()],
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
        if !self.serial_protocols.is_empty() {
            capabilities.push("transport:serial".to_string());
        }
        if self.mqtt_uplink_enabled {
            capabilities.push("uplink:mqtt".to_string());
        }
        capabilities.push(format!("local-store:{}", self.local_store_backend));
        capabilities
    }
}
