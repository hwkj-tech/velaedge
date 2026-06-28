use std::collections::BTreeMap;

use anyhow::{bail, Result};
use edge_core::{
    DataQuality, DeviceShadow, EdgeConfigPackage, ProtocolConnection, ProtocolType,
    TelemetryPointMapping, TelemetrySample, TelemetryValue,
};

use crate::{
    publish_mqtt_samples, CollectionReport, ConfiguredMqttCollectionReport, ModbusRtuAdapter,
    MqttPublisher, ProtocolAdapter, SerialBusFactory,
};

pub struct ConfiguredEdgeRuntime<F> {
    package: EdgeConfigPackage,
    serial_bus_factory: F,
    shadows: BTreeMap<String, DeviceShadow>,
}

impl<F> ConfiguredEdgeRuntime<F>
where
    F: SerialBusFactory,
{
    pub fn new(package: EdgeConfigPackage, serial_bus_factory: F) -> Result<Self> {
        if package.edge_id.trim().is_empty() {
            bail!("edge id is required");
        }
        if package.version.trim().is_empty() {
            bail!("config version is required");
        }

        let mut shadows = BTreeMap::new();
        for device in &package.devices {
            shadows.insert(
                device.device_id.clone(),
                DeviceShadow::new(&package.edge_id, &device.device_id),
            );
        }

        Ok(Self {
            package,
            serial_bus_factory,
            shadows,
        })
    }

    pub async fn collect_once(&mut self) -> Result<CollectionReport> {
        let samples = self.collect_samples_once().await?;
        Ok(CollectionReport {
            samples_collected: samples.len(),
        })
    }

    pub async fn collect_once_and_publish_mqtt<P>(
        &mut self,
        publisher: &mut P,
    ) -> Result<ConfiguredMqttCollectionReport>
    where
        P: MqttPublisher + ?Sized,
    {
        let samples = self.collect_samples_once().await?;
        let mqtt_messages_published =
            publish_mqtt_samples(&self.package, &samples, publisher).await?;
        Ok(ConfiguredMqttCollectionReport {
            collection: CollectionReport {
                samples_collected: samples.len(),
            },
            mqtt_messages_published,
        })
    }

    pub fn reported_version(&self) -> &str {
        &self.package.version
    }

    pub fn shadow(&self, device_id: &str) -> Option<&DeviceShadow> {
        self.shadows.get(device_id)
    }

    async fn collect_samples_once(&mut self) -> Result<Vec<TelemetrySample>> {
        let mut samples = Vec::new();
        let connections = self.package.protocol_connections.clone();
        for connection in connections {
            let mappings = self.mappings_for_connection(&connection);
            if mappings.is_empty() {
                continue;
            }

            let mut connection_samples = match connection.protocol {
                ProtocolType::Simulated => collect_simulated_samples(&mappings),
                ProtocolType::ModbusRtu => {
                    let bus = self.serial_bus_factory.open(&connection)?;
                    let mut adapter = ModbusRtuAdapter::new(connection, mappings, bus);
                    adapter.read_telemetry().await?
                }
                unsupported => bail!("unsupported runtime protocol: {unsupported:?}"),
            };
            for sample in &connection_samples {
                if let Some(shadow) = self.shadows.get_mut(&sample.device_id) {
                    shadow.update(sample.clone());
                }
            }
            samples.append(&mut connection_samples);
        }

        Ok(samples)
    }

    fn mappings_for_connection(
        &self,
        connection: &ProtocolConnection,
    ) -> Vec<TelemetryPointMapping> {
        self.package
            .point_mappings
            .iter()
            .filter(|mapping| mapping.protocol_connection_id == connection.connection_id)
            .cloned()
            .collect()
    }
}

fn collect_simulated_samples(mappings: &[TelemetryPointMapping]) -> Vec<TelemetrySample> {
    mappings
        .iter()
        .map(|mapping| {
            TelemetrySample::new(
                &mapping.device_id,
                &mapping.point_id,
                TelemetryValue::Float(1.0),
                DataQuality::Good,
                chrono::Utc::now(),
            )
        })
        .collect()
}
