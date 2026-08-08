use std::{collections::BTreeSet, sync::Arc};

use anyhow::{bail, Result};
use edge_core::{EdgeConfigPackage, MqttUplinkConfig};
use tokio::task::JoinHandle;

use crate::{
    ConfiguredEdgeRuntime, MqttCommandSubscriber, MultiBrokerMqttPublisher,
    ProtocolCircuitBreakerRegistry, RocksEdgeRuntimeStore, TokioSerialBusFactory,
};

pub struct CommandRuntimeService {
    config_version: String,
    enabled_flow_count: usize,
    task: JoinHandle<()>,
}

impl CommandRuntimeService {
    pub async fn start(
        package: EdgeConfigPackage,
        store: Arc<RocksEdgeRuntimeStore>,
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

        let mut subscriber = MqttCommandSubscriber::connect_from_package(&package).await?;
        let reply_uplinks = command_reply_uplinks(&package);
        let mut publisher = MultiBrokerMqttPublisher::connect_from_uplinks(&reply_uplinks)?;
        let mut execution_package = package.clone();
        for uplink in &mut execution_package.mqtt_uplinks {
            if let Some(reply_uplink) = reply_uplinks
                .iter()
                .find(|reply_uplink| reply_uplink.sink_id == uplink.sink_id)
            {
                uplink.client_id = reply_uplink.client_id.clone();
            }
        }
        let mut runtime = ConfiguredEdgeRuntime::new_with_circuit_breakers(
            execution_package,
            TokioSerialBusFactory,
            circuit_breakers,
        )?;
        let config_version = package.version.clone();
        let edge_id = package.edge_id.clone();
        let task = tokio::spawn(async move {
            while let Some(message) = subscriber.recv().await {
                match runtime
                    .execute_mqtt_command_message_with_store(&message, &store, &mut publisher)
                    .await
                {
                    Ok(reports) => {
                        for report in reports {
                            tracing::info!(
                                edge_id = %edge_id,
                                flow_id = %report.flow_id,
                                command_id = %report.command_id,
                                duplicate = report.duplicate,
                                status = ?report.status,
                                write_count = report.writes.len(),
                                "MQTT command processed"
                            );
                        }
                    }
                    Err(error) => tracing::warn!(
                        edge_id = %edge_id,
                        sink_id = %message.sink_id,
                        topic = %message.topic,
                        error = %error,
                        "MQTT command rejected or failed"
                    ),
                }
            }
            tracing::warn!(edge_id = %edge_id, "MQTT command subscriber stopped");
        });

        Ok(Self {
            config_version,
            enabled_flow_count,
            task,
        })
    }

    pub fn config_version(&self) -> &str {
        &self.config_version
    }

    pub fn enabled_flow_count(&self) -> usize {
        self.enabled_flow_count
    }

    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
}

impl Drop for CommandRuntimeService {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn command_reply_uplinks(package: &EdgeConfigPackage) -> Vec<MqttUplinkConfig> {
    let sink_ids = package
        .command_flows
        .iter()
        .filter(|flow| flow.enabled)
        .map(|flow| flow.mqtt_connection_id.as_str())
        .collect::<BTreeSet<_>>();
    package
        .mqtt_uplinks
        .iter()
        .filter(|uplink| sink_ids.contains(uplink.sink_id.as_str()))
        .cloned()
        .map(|mut uplink| {
            uplink.client_id = format!("{}-command-replies", uplink.client_id);
            uplink
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use edge_core::{CommandFlowConfig, EdgeConfigPackage, MqttUplinkConfig};

    use super::command_reply_uplinks;

    #[test]
    fn command_reply_publishers_use_dedicated_client_ids_and_only_referenced_sinks() {
        let package = EdgeConfigPackage::new("edge-1", "v1")
            .with_mqtt_uplink(MqttUplinkConfig::velamq(
                "commands",
                "mqtt://127.0.0.1:1883",
                "runtime-1",
            ))
            .with_mqtt_uplink(MqttUplinkConfig::velamq(
                "telemetry",
                "mqtt://127.0.0.1:1883",
                "runtime-telemetry",
            ))
            .with_command_flow(CommandFlowConfig::new(
                "flow-1",
                "写点位",
                "commands",
                "edge/command",
                "edge/reply",
            ));

        let uplinks = command_reply_uplinks(&package);

        assert_eq!(uplinks.len(), 1);
        assert_eq!(uplinks[0].sink_id, "commands");
        assert_eq!(uplinks[0].client_id, "runtime-1-command-replies");
    }
}
