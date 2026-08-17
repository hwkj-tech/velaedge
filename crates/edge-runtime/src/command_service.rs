use anyhow::{bail, Result};
use edge_core::EdgeConfigPackage;

use crate::{
    CommandExecutionReport, ConfiguredEdgeRuntime, MqttCommandMessage, MqttPublisher,
    ProtocolCircuitBreakerRegistry, RocksEdgeRuntimeStore, TokioSerialBusFactory,
};

/// Stateful command processor that shares the Runtime's persistent MQTT session.
pub struct CommandRuntimeService {
    config_version: String,
    edge_id: String,
    enabled_flow_count: usize,
    runtime: ConfiguredEdgeRuntime<TokioSerialBusFactory>,
}

impl CommandRuntimeService {
    pub fn from_package(
        package: EdgeConfigPackage,
        circuit_breakers: ProtocolCircuitBreakerRegistry,
    ) -> Result<Self> {
        let enabled_flow_count = package
            .command_flows
            .iter()
            .filter(|flow| flow.enabled)
            .count();
        if enabled_flow_count == 0 {
            bail!("at least one enabled command flow is required");
        }

        let config_version = package.version.clone();
        let edge_id = package.edge_id.clone();
        let runtime = ConfiguredEdgeRuntime::new_with_circuit_breakers(
            package,
            TokioSerialBusFactory,
            circuit_breakers,
        )?;
        Ok(Self {
            config_version,
            edge_id,
            enabled_flow_count,
            runtime,
        })
    }

    pub async fn process_message<P>(
        &mut self,
        message: &MqttCommandMessage,
        store: &RocksEdgeRuntimeStore,
        publisher: &mut P,
    ) -> Result<Vec<CommandExecutionReport>>
    where
        P: MqttPublisher + ?Sized,
    {
        let reports = self
            .runtime
            .execute_mqtt_command_message_with_store(message, store, publisher)
            .await?;
        for report in &reports {
            tracing::info!(
                edge_id = %self.edge_id,
                flow_id = %report.flow_id,
                command_id = %report.command_id,
                duplicate = report.duplicate,
                status = ?report.status,
                write_count = report.writes.len(),
                "MQTT command processed"
            );
        }
        Ok(reports)
    }

    pub fn config_version(&self) -> &str {
        &self.config_version
    }

    pub fn enabled_flow_count(&self) -> usize {
        self.enabled_flow_count
    }
}

#[cfg(test)]
mod tests {
    use edge_core::{
        CommandFlowConfig, CommandGraphEdge, CommandGraphNode, CommandGraphNodeKind,
        DeviceInstance, EdgeConfigPackage, MqttUplinkConfig, PointAccess, PointAddress,
        ProtocolConnection, TelemetryPointMapping, TelemetryType,
    };

    use super::CommandRuntimeService;
    use crate::ProtocolCircuitBreakerRegistry;

    #[test]
    fn command_processor_uses_package_without_creating_dedicated_mqtt_clients() {
        let flow =
            CommandFlowConfig::new("flow-1", "写点位", "commands", "edge/command", "edge/reply")
                .with_node(CommandGraphNode::new(
                    "input",
                    CommandGraphNodeKind::MqttInput,
                    "input",
                ))
                .with_node(
                    CommandGraphNode::new("write", CommandGraphNodeKind::PointWrite, "write")
                        .with_ref("setpoint"),
                )
                .with_node(CommandGraphNode::new(
                    "reply",
                    CommandGraphNodeKind::MqttReply,
                    "reply",
                ))
                .with_edge(CommandGraphEdge::new("input-write", "input", "write"))
                .with_edge(CommandGraphEdge::new("write-reply", "write", "reply"));
        let package = EdgeConfigPackage::new("edge-1", "v1")
            .with_device(DeviceInstance::new("device-1", "device"))
            .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
            .with_point_mapping(
                TelemetryPointMapping::new(
                    "setpoint",
                    "device-1",
                    "device.setpoint",
                    "sim-main",
                    PointAddress::simulated("setpoint"),
                    TelemetryType::Float,
                )
                .with_access(PointAccess::ReadWrite),
            )
            .with_mqtt_uplink(MqttUplinkConfig::velamq(
                "commands",
                "mqtt://127.0.0.1:1883",
                "edge-1",
            ))
            .with_command_flow(flow);

        let service =
            CommandRuntimeService::from_package(package, ProtocolCircuitBreakerRegistry::default())
                .unwrap();

        assert_eq!(service.config_version(), "v1");
        assert_eq!(service.enabled_flow_count(), 1);
    }
}
