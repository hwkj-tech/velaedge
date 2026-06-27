use edge_core::{
    DiscoveredPoint, DiscoveryReport, PointAddress, PointMappingSuggestion, TelemetryType,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimulatedSerialDiscovery {
    job_id: String,
    protocol_connection_id: String,
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
