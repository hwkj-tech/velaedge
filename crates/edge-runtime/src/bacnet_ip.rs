use std::collections::{BTreeMap, BTreeSet};
use std::net::Ipv4Addr;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use bacnet_client::client::{BACnetClient, ReceivedCOVNotification};
use bacnet_encoding::primitives::{decode_application_value, encode_property_value};
use bacnet_services::common::PropertyReference;
use bacnet_services::rpm::{ReadAccessSpecification, ReadPropertyMultipleACK};
use bacnet_transport::bip::{BipTransport, ForeignDeviceConfig};
use bacnet_types::enums::{ObjectType, PropertyIdentifier};
use bacnet_types::primitives::{ObjectIdentifier, PropertyValue};
use bytes::BytesMut;
use chrono::Utc;
use edge_core::{
    parse_bacnet_ip_endpoint, parse_bacnet_point_address, validate_bacnet_point,
    BacnetPointAddress, DataQuality, DataQualityCode, ProtocolConnection, ProtocolType,
    TelemetryPointMapping, TelemetrySample, TelemetryType, TelemetryValue,
};
use tokio::sync::broadcast::error::TryRecvError;
use tokio::time::{sleep, Instant};

use crate::{ProtocolAdapter, ProtocolCommandAdapter, ProtocolWriteResult};

type PropertyKey = (u32, u32, u32, u32, Option<u32>);
type CovObjectKey = (u32, u32, u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BacnetCovRuntimeMetrics {
    pub active_subscriptions: usize,
    pub notifications_received: u64,
    pub subscription_failures: u64,
    pub fallback_polls: u64,
}

#[derive(Clone, Copy, Debug)]
struct CovSubscriptionState {
    process_id: u32,
    renew_at: Instant,
}

pub struct BacnetIpAdapter {
    connection: ProtocolConnection,
    mappings: Vec<TelemetryPointMapping>,
    client: Option<BACnetClient<BipTransport>>,
    connection_generation: u64,
    cov_receiver: Option<tokio::sync::broadcast::Receiver<ReceivedCOVNotification>>,
    cov_subscriptions: BTreeMap<CovObjectKey, CovSubscriptionState>,
    cov_next_process_id: u32,
    cov_initialized: bool,
    cov_last_fallback_poll: Option<Instant>,
    cov_force_fallback: bool,
    cov_metrics: BacnetCovRuntimeMetrics,
}

impl BacnetIpAdapter {
    pub fn new(
        connection: ProtocolConnection,
        mappings: Vec<TelemetryPointMapping>,
    ) -> Result<Self> {
        if connection.protocol != ProtocolType::BacnetIp {
            bail!("BACnet/IP adapter requires a BACnet/IP connection");
        }
        connection.validate().map_err(anyhow::Error::msg)?;
        validate_mappings(&connection, &mappings)?;
        Ok(Self {
            connection,
            mappings,
            client: None,
            connection_generation: 0,
            cov_receiver: None,
            cov_subscriptions: BTreeMap::new(),
            cov_next_process_id: 1,
            cov_initialized: false,
            cov_last_fallback_poll: None,
            cov_force_fallback: false,
            cov_metrics: BacnetCovRuntimeMetrics::default(),
        })
    }

    pub fn set_mappings(&mut self, mappings: Vec<TelemetryPointMapping>) -> Result<()> {
        validate_mappings(&self.connection, &mappings)?;
        if self.mappings != mappings {
            self.reset_client();
        }
        self.mappings = mappings;
        Ok(())
    }

    pub fn connection_generation(&self) -> u64 {
        self.connection_generation
    }

    pub fn cov_runtime_metrics(&self) -> BacnetCovRuntimeMetrics {
        BacnetCovRuntimeMetrics {
            active_subscriptions: self.cov_subscriptions.len(),
            ..self.cov_metrics
        }
    }

    fn reset_client(&mut self) {
        self.client = None;
        self.cov_receiver = None;
        self.cov_subscriptions.clear();
        self.cov_initialized = false;
        self.cov_last_fallback_poll = None;
        self.cov_force_fallback = true;
    }

    async fn client(&mut self) -> Result<&BACnetClient<BipTransport>> {
        if self.client.is_none() {
            let settings = self
                .connection
                .bacnet_ip
                .as_ref()
                .context("BACnet/IP settings are required")?;
            let bind_address = settings
                .bind_address
                .parse::<Ipv4Addr>()
                .context("invalid BACnet/IP bind address")?;
            let broadcast_address = settings
                .broadcast_address
                .parse::<Ipv4Addr>()
                .context("invalid BACnet/IP broadcast address")?;
            let mut transport =
                BipTransport::new(bind_address, settings.local_port, broadcast_address);
            if let Some(foreign_device) = &settings.foreign_device {
                let bbmd = parse_bacnet_ip_endpoint(&foreign_device.bbmd_address)
                    .map_err(anyhow::Error::msg)
                    .context("invalid BACnet/IP BBMD address")?;
                transport.register_as_foreign_device(ForeignDeviceConfig {
                    bbmd_ip: *bbmd.ip(),
                    bbmd_port: bbmd.port(),
                    ttl: foreign_device.ttl_seconds,
                });
            }
            let client = BACnetClient::<BipTransport>::generic_builder()
                .transport(transport)
                .apdu_timeout_ms(settings.apdu_timeout_ms)
                .apdu_retries(settings.apdu_retries)
                .max_apdu_length(settings.max_apdu_length)
                .build()
                .await
                .context("failed to start BACnet/IP client")?;
            self.cov_receiver = self
                .connection
                .bacnet_ip
                .as_ref()
                .and_then(|settings| settings.cov.as_ref())
                .map(|_| client.cov_notifications());
            self.connection_generation = self.connection_generation.saturating_add(1);
            self.client = Some(client);
        }
        Ok(self.client.as_ref().expect("BACnet/IP client initialized"))
    }
}

#[async_trait]
impl ProtocolAdapter for BacnetIpAdapter {
    async fn read_telemetry(&mut self) -> Result<Vec<TelemetrySample>> {
        let reads = build_reads(&self.connection.connection_id, &self.mappings)?;
        let direct_mac = self
            .connection
            .endpoint
            .as_deref()
            .map(parse_bacnet_mac)
            .transpose()?;
        let discovery_timeout = Duration::from_millis(
            self.connection
                .bacnet_ip
                .as_ref()
                .expect("validated BACnet/IP settings")
                .discovery_timeout_ms,
        );
        let cov = self
            .connection
            .bacnet_ip
            .as_ref()
            .expect("validated BACnet/IP settings")
            .cov
            .clone();
        if let Some(cov) = cov {
            return self
                .read_cov_telemetry(&reads, direct_mac.as_deref(), discovery_timeout, &cov)
                .await;
        }

        let result = self
            .execute_snapshot(&reads, direct_mac.as_deref(), discovery_timeout)
            .await;
        if result.is_err() {
            self.reset_client();
        }
        let values = result?;
        Ok(samples_from_snapshot(&reads, &values))
    }
}

#[async_trait]
impl ProtocolCommandAdapter for BacnetIpAdapter {
    async fn write_point(
        &mut self,
        mapping: &TelemetryPointMapping,
        value: TelemetryValue,
    ) -> Result<ProtocolWriteResult> {
        if mapping.protocol_connection_id != self.connection.connection_id {
            bail!(
                "BACnet/IP point {} references connection {} instead of {}",
                mapping.point_id,
                mapping.protocol_connection_id,
                self.connection.connection_id
            );
        }
        if !mapping.access.is_writable() {
            bail!("BACnet/IP point {} is read-only", mapping.point_id);
        }
        let address = validate_bacnet_point(
            &mapping.address,
            mapping.value_type,
            mapping.access,
            mapping.bacnet,
        )
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("invalid BACnet/IP command point {}", mapping.point_id))?;
        let priority = mapping.bacnet.unwrap_or_default();

        let object_identifier = ObjectIdentifier::new_addressable(
            ObjectType::from_raw(address.object_type),
            address.object_instance,
        )
        .map_err(anyhow::Error::new)?;
        let property_identifier = PropertyIdentifier::from_raw(address.property_identifier);
        let property_value = command_property_value(mapping, address, &value)?;
        let mut encoded_value = BytesMut::new();
        encode_property_value(&mut encoded_value, &property_value)
            .context("encode BACnet command property value")?;
        let direct_mac = self
            .connection
            .endpoint
            .as_deref()
            .map(parse_bacnet_mac)
            .transpose()?;
        let discovery_timeout = Duration::from_millis(
            self.connection
                .bacnet_ip
                .as_ref()
                .expect("validated BACnet/IP settings")
                .discovery_timeout_ms,
        );
        let write = async {
            let client = self.client().await?;
            ensure_device(
                client,
                address.device_instance,
                direct_mac.as_deref(),
                discovery_timeout,
            )
            .await?;
            client
                .write_property_to_device(
                    address.device_instance,
                    object_identifier,
                    property_identifier,
                    address.array_index,
                    encoded_value.to_vec(),
                    Some(priority.write_priority),
                )
                .await
                .with_context(|| {
                    format!(
                        "BACnet/IP WriteProperty failed for point {} at priority {}",
                        mapping.point_id, priority.write_priority
                    )
                })
        }
        .await;
        if write.is_err() {
            self.reset_client();
        }
        write?;

        Ok(ProtocolWriteResult {
            point_id: mapping.point_id.clone(),
            value,
            verified: false,
            readback_value: None,
        })
    }
}

impl BacnetIpAdapter {
    async fn execute_snapshot(
        &mut self,
        reads: &ReadPlan,
        direct_mac: Option<&[u8]>,
        discovery_timeout: Duration,
    ) -> Result<BTreeMap<PropertyKey, Result<PropertyValue, String>>> {
        let client = self.client().await?;
        execute_reads(client, reads, direct_mac, discovery_timeout).await
    }

    async fn read_cov_telemetry(
        &mut self,
        reads: &ReadPlan,
        direct_mac: Option<&[u8]>,
        discovery_timeout: Duration,
        cov: &edge_core::BacnetCovSettings,
    ) -> Result<Vec<TelemetrySample>> {
        if let Err(error) = self
            .maintain_cov_subscriptions(reads, direct_mac, discovery_timeout, cov)
            .await
        {
            self.cov_metrics.subscription_failures =
                self.cov_metrics.subscription_failures.saturating_add(1);
            self.cov_force_fallback = true;
            tracing::warn!(
                connection_id = %self.connection.connection_id,
                error = %error,
                "BACnet COV subscription unavailable; using fallback polling"
            );
        }

        let (changed, notifications, stream_unhealthy) = self.drain_cov_notifications();
        self.cov_metrics.notifications_received = self
            .cov_metrics
            .notifications_received
            .saturating_add(notifications);
        self.cov_force_fallback |= stream_unhealthy;

        let now = Instant::now();
        let fallback_interval = Duration::from_millis(cov.fallback_poll_interval_ms);
        let fallback_due = self
            .cov_last_fallback_poll
            .is_none_or(|last| now.saturating_duration_since(last) >= fallback_interval);
        let snapshot_required = !self.cov_initialized || self.cov_force_fallback || fallback_due;

        if snapshot_required {
            let result = self
                .execute_snapshot(reads, direct_mac, discovery_timeout)
                .await;
            if result.is_err() {
                self.reset_client();
            }
            let mut values = result?;
            values.extend(changed);
            self.cov_initialized = true;
            self.cov_last_fallback_poll = Some(now);
            self.cov_force_fallback = false;
            self.cov_metrics.fallback_polls = self.cov_metrics.fallback_polls.saturating_add(1);
            return Ok(samples_from_snapshot(reads, &values));
        }

        Ok(reads
            .targets
            .iter()
            .filter_map(|target| {
                changed
                    .get(&target.key)
                    .map(|result| sample_from_result(target, Some(result)))
            })
            .collect())
    }

    async fn maintain_cov_subscriptions(
        &mut self,
        reads: &ReadPlan,
        direct_mac: Option<&[u8]>,
        discovery_timeout: Duration,
        cov: &edge_core::BacnetCovSettings,
    ) -> Result<()> {
        let now = Instant::now();
        let margin_seconds = u64::from((cov.lifetime_seconds / 3).clamp(5, 30));
        let renew_after = Duration::from_secs(u64::from(cov.lifetime_seconds))
            .saturating_sub(Duration::from_secs(margin_seconds));

        for key in cov_objects(reads) {
            let existing = self.cov_subscriptions.get(&key).copied();
            if existing.is_some_and(|state| state.renew_at > now) {
                continue;
            }
            let process_id = existing
                .map(|state| state.process_id)
                .unwrap_or_else(|| self.allocate_cov_process_id());
            {
                let client = self.client().await?;
                ensure_device(client, key.0, direct_mac, discovery_timeout).await?;
                let object = ObjectIdentifier::new_addressable(ObjectType::from_raw(key.1), key.2)
                    .map_err(anyhow::Error::new)?;
                client
                    .subscribe_cov_to_device(
                        key.0,
                        process_id,
                        object,
                        cov.confirmed_notifications,
                        Some(cov.lifetime_seconds),
                    )
                    .await
                    .with_context(|| {
                        format!(
                            "failed to subscribe BACnet COV device={} object={}:{}",
                            key.0, key.1, key.2
                        )
                    })?;
            }
            self.cov_subscriptions.insert(
                key,
                CovSubscriptionState {
                    process_id,
                    renew_at: now + renew_after,
                },
            );
        }
        Ok(())
    }

    fn allocate_cov_process_id(&mut self) -> u32 {
        let process_id = self.cov_next_process_id.max(1);
        self.cov_next_process_id = process_id.wrapping_add(1).max(1);
        process_id
    }

    fn drain_cov_notifications(
        &mut self,
    ) -> (
        BTreeMap<PropertyKey, Result<PropertyValue, String>>,
        u64,
        bool,
    ) {
        let by_process = self
            .cov_subscriptions
            .iter()
            .map(|(key, state)| (state.process_id, *key))
            .collect::<BTreeMap<_, _>>();
        let mut values = BTreeMap::new();
        let mut notifications = 0_u64;
        let mut unhealthy = false;
        let Some(receiver) = self.cov_receiver.as_mut() else {
            return (values, notifications, true);
        };

        loop {
            match receiver.try_recv() {
                Ok(received) => {
                    let Some(object) =
                        by_process.get(&received.notification.subscriber_process_identifier)
                    else {
                        continue;
                    };
                    if received
                        .notification
                        .monitored_object_identifier
                        .object_type()
                        .to_raw()
                        != object.1
                        || received
                            .notification
                            .monitored_object_identifier
                            .instance_number()
                            != object.2
                    {
                        continue;
                    }
                    notifications = notifications.saturating_add(1);
                    collect_cov_values(&mut values, *object, &received);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Lagged(_)) => unhealthy = true,
                Err(TryRecvError::Closed) => {
                    unhealthy = true;
                    break;
                }
            }
        }
        (values, notifications, unhealthy)
    }
}

#[derive(Clone)]
struct ReadTarget {
    mapping: TelemetryPointMapping,
    key: PropertyKey,
}

struct ReadPlan {
    targets: Vec<ReadTarget>,
    by_device: BTreeMap<u32, Vec<ReadAccessSpecification>>,
}

fn command_property_value(
    mapping: &TelemetryPointMapping,
    address: BacnetPointAddress,
    value: &TelemetryValue,
) -> Result<PropertyValue> {
    match (address.object_type, value) {
        (1 | 2, TelemetryValue::Float(value)) => {
            if !value.is_finite() || *value < f64::from(f32::MIN) || *value > f64::from(f32::MAX) {
                bail!(
                    "BACnet/IP analog value for point {} must fit a finite REAL",
                    mapping.point_id
                );
            }
            Ok(PropertyValue::Real(*value as f32))
        }
        (4 | 5, TelemetryValue::Boolean(value)) => Ok(PropertyValue::Enumerated(u32::from(*value))),
        (14 | 19, TelemetryValue::Integer(value)) => {
            let value = u32::try_from(*value).with_context(|| {
                format!(
                    "BACnet/IP multi-state value for point {} must be between 1 and {}",
                    mapping.point_id,
                    u32::MAX
                )
            })?;
            if value == 0 {
                bail!(
                    "BACnet/IP multi-state value for point {} must be greater than zero",
                    mapping.point_id
                );
            }
            Ok(PropertyValue::Unsigned(u64::from(value)))
        }
        _ => bail!(
            "BACnet/IP command value does not match point {} object and telemetry type",
            mapping.point_id
        ),
    }
}

fn validate_mappings(
    connection: &ProtocolConnection,
    mappings: &[TelemetryPointMapping],
) -> Result<()> {
    for mapping in mappings {
        if mapping.protocol_connection_id != connection.connection_id {
            bail!(
                "BACnet/IP point {} references connection {} instead of {}",
                mapping.point_id,
                mapping.protocol_connection_id,
                connection.connection_id
            );
        }
        if mapping.address.kind != "bacnet_object_property" {
            bail!(
                "BACnet/IP point {} requires bacnet_object_property address kind",
                mapping.point_id
            );
        }
        validate_bacnet_point(
            &mapping.address,
            mapping.value_type,
            mapping.access,
            mapping.bacnet,
        )
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("invalid BACnet/IP point {}", mapping.point_id))?;
    }
    Ok(())
}

fn build_reads(connection_id: &str, mappings: &[TelemetryPointMapping]) -> Result<ReadPlan> {
    let mut targets = Vec::new();
    let mut properties = BTreeMap::<u32, BTreeMap<(u32, u32), BTreeSet<(u32, Option<u32>)>>>::new();
    for mapping in mappings
        .iter()
        .filter(|mapping| mapping.protocol_connection_id == connection_id)
    {
        let address = parse_bacnet_point_address(&mapping.address.value)
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("invalid BACnet/IP address for point {}", mapping.point_id))?;
        let key = property_key(address);
        properties
            .entry(address.device_instance)
            .or_default()
            .entry((address.object_type, address.object_instance))
            .or_default()
            .insert((address.property_identifier, address.array_index));
        targets.push(ReadTarget {
            mapping: mapping.clone(),
            key,
        });
    }

    let mut by_device = BTreeMap::new();
    for (device, objects) in properties {
        let specs = objects
            .into_iter()
            .map(|((object_type, object_instance), properties)| {
                let object_identifier = ObjectIdentifier::new_addressable(
                    ObjectType::from_raw(object_type),
                    object_instance,
                )
                .map_err(anyhow::Error::new)?;
                Ok(ReadAccessSpecification {
                    object_identifier,
                    list_of_property_references: properties
                        .into_iter()
                        .map(|(property, array_index)| PropertyReference {
                            property_identifier: PropertyIdentifier::from_raw(property),
                            property_array_index: array_index,
                        })
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        by_device.insert(device, specs);
    }
    Ok(ReadPlan { targets, by_device })
}

fn property_key(address: BacnetPointAddress) -> PropertyKey {
    (
        address.device_instance,
        address.object_type,
        address.object_instance,
        address.property_identifier,
        address.array_index,
    )
}

async fn execute_reads(
    client: &BACnetClient<BipTransport>,
    plan: &ReadPlan,
    direct_mac: Option<&[u8]>,
    discovery_timeout: Duration,
) -> Result<BTreeMap<PropertyKey, Result<PropertyValue, String>>> {
    let mut values = BTreeMap::new();
    for (device_instance, specs) in &plan.by_device {
        ensure_device(client, *device_instance, direct_mac, discovery_timeout).await?;
        match client
            .read_property_multiple_from_device(*device_instance, specs.clone())
            .await
        {
            Ok(ack) => collect_rpm_values(&mut values, *device_instance, ack),
            Err(batch_error) => {
                let fallback = read_properties_individually(client, *device_instance, specs).await;
                if fallback.is_empty() {
                    return Err(anyhow!(batch_error).context(format!(
                        "BACnet/IP ReadPropertyMultiple failed for device {device_instance}"
                    )));
                }
                values.extend(fallback);
            }
        }
    }
    Ok(values)
}

async fn ensure_device(
    client: &BACnetClient<BipTransport>,
    device_instance: u32,
    direct_mac: Option<&[u8]>,
    discovery_timeout: Duration,
) -> Result<()> {
    if client.get_device(device_instance).await.is_some() {
        return Ok(());
    }
    if let Some(mac) = direct_mac {
        client
            .add_device(device_instance, mac)
            .await
            .context("failed to register configured BACnet/IP device")?;
        return Ok(());
    }

    client
        .who_is(Some(device_instance), Some(device_instance))
        .await
        .context("failed to broadcast BACnet Who-Is")?;
    let deadline = Instant::now() + discovery_timeout;
    while Instant::now() < deadline {
        if client.get_device(device_instance).await.is_some() {
            return Ok(());
        }
        sleep(Duration::from_millis(25)).await;
    }
    bail!("BACnet/IP device {device_instance} was not discovered before timeout")
}

fn parse_bacnet_mac(endpoint: &str) -> Result<Vec<u8>> {
    let endpoint = parse_bacnet_ip_endpoint(endpoint).map_err(anyhow::Error::msg)?;
    let mut mac = endpoint.ip().octets().to_vec();
    mac.extend_from_slice(&endpoint.port().to_be_bytes());
    Ok(mac)
}

fn collect_rpm_values(
    values: &mut BTreeMap<PropertyKey, Result<PropertyValue, String>>,
    device_instance: u32,
    ack: ReadPropertyMultipleACK,
) {
    for object in ack.list_of_read_access_results {
        for result in object.list_of_results {
            let key = (
                device_instance,
                object.object_identifier.object_type().to_raw(),
                object.object_identifier.instance_number(),
                result.property_identifier.to_raw(),
                result.property_array_index,
            );
            let value = match (result.property_value, result.error) {
                (Some(bytes), _) => decode_property_value(&bytes),
                (_, Some((class, code))) => Err(format!(
                    "BACnet property error class={} code={}",
                    class.to_raw(),
                    code.to_raw()
                )),
                _ => Err("BACnet property response contained no value".to_string()),
            };
            values.insert(key, value);
        }
    }
}

async fn read_properties_individually(
    client: &BACnetClient<BipTransport>,
    device_instance: u32,
    specs: &[ReadAccessSpecification],
) -> BTreeMap<PropertyKey, Result<PropertyValue, String>> {
    let mut values = BTreeMap::new();
    for spec in specs {
        for property in &spec.list_of_property_references {
            let key = (
                device_instance,
                spec.object_identifier.object_type().to_raw(),
                spec.object_identifier.instance_number(),
                property.property_identifier.to_raw(),
                property.property_array_index,
            );
            let value = client
                .read_property_from_device(
                    device_instance,
                    spec.object_identifier,
                    property.property_identifier,
                    property.property_array_index,
                )
                .await
                .map_err(|error| error.to_string())
                .and_then(|ack| decode_property_value(&ack.property_value));
            values.insert(key, value);
        }
    }
    values
}

fn cov_objects(reads: &ReadPlan) -> BTreeSet<CovObjectKey> {
    reads
        .by_device
        .iter()
        .flat_map(|(device, specs)| {
            specs.iter().map(|spec| {
                (
                    *device,
                    spec.object_identifier.object_type().to_raw(),
                    spec.object_identifier.instance_number(),
                )
            })
        })
        .collect()
}

fn collect_cov_values(
    values: &mut BTreeMap<PropertyKey, Result<PropertyValue, String>>,
    object: CovObjectKey,
    received: &ReceivedCOVNotification,
) {
    for property in &received.notification.list_of_values {
        values.insert(
            (
                object.0,
                object.1,
                object.2,
                property.property_identifier.to_raw(),
                property.property_array_index,
            ),
            decode_property_value(&property.value),
        );
    }
}

fn samples_from_snapshot(
    reads: &ReadPlan,
    values: &BTreeMap<PropertyKey, Result<PropertyValue, String>>,
) -> Vec<TelemetrySample> {
    reads
        .targets
        .iter()
        .map(|target| sample_from_result(target, values.get(&target.key)))
        .collect()
}

fn decode_property_value(bytes: &[u8]) -> Result<PropertyValue, String> {
    let (value, consumed) =
        decode_application_value(bytes, 0).map_err(|error| error.to_string())?;
    if consumed != bytes.len() {
        return Err("BACnet property value contains trailing application data".to_string());
    }
    Ok(value)
}

fn sample_from_result(
    target: &ReadTarget,
    result: Option<&Result<PropertyValue, String>>,
) -> TelemetrySample {
    let (value, quality) = match result {
        Some(Ok(value)) => match telemetry_value(value, target.mapping.value_type) {
            Ok(value) => (value, DataQualityCode::Good),
            Err(_) => (
                default_value(target.mapping.value_type),
                DataQualityCode::BadDecode,
            ),
        },
        Some(Err(_)) | None => (
            default_value(target.mapping.value_type),
            DataQualityCode::BadProtocol,
        ),
    };
    TelemetrySample::new(
        &target.mapping.device_id,
        &target.mapping.point_id,
        value,
        DataQuality::Good,
        Utc::now(),
    )
    .with_quality_code(quality)
}

fn telemetry_value(value: &PropertyValue, value_type: TelemetryType) -> Result<TelemetryValue> {
    match value_type {
        TelemetryType::Float => numeric_f64(value)
            .map(TelemetryValue::Float)
            .context("BACnet property is not numeric"),
        TelemetryType::Integer => numeric_i64(value)
            .map(TelemetryValue::Integer)
            .context("BACnet property is not an integer or exceeds i64"),
        TelemetryType::Boolean => match value {
            PropertyValue::Boolean(value) => Ok(TelemetryValue::Boolean(*value)),
            PropertyValue::Unsigned(value) => Ok(TelemetryValue::Boolean(*value != 0)),
            PropertyValue::Signed(value) => Ok(TelemetryValue::Boolean(*value != 0)),
            PropertyValue::Enumerated(value) => Ok(TelemetryValue::Boolean(*value != 0)),
            _ => bail!("BACnet property is not boolean-compatible"),
        },
        TelemetryType::Text => Ok(TelemetryValue::Text(property_text(value))),
    }
}

fn numeric_f64(value: &PropertyValue) -> Option<f64> {
    match value {
        PropertyValue::Unsigned(value) => Some(*value as f64),
        PropertyValue::Signed(value) => Some(f64::from(*value)),
        PropertyValue::Real(value) => Some(f64::from(*value)),
        PropertyValue::Double(value) => Some(*value),
        PropertyValue::Enumerated(value) => Some(f64::from(*value)),
        PropertyValue::Boolean(value) => Some(if *value { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn numeric_i64(value: &PropertyValue) -> Option<i64> {
    match value {
        PropertyValue::Unsigned(value) => i64::try_from(*value).ok(),
        PropertyValue::Signed(value) => Some(i64::from(*value)),
        PropertyValue::Real(value) if value.is_finite() => Some(*value as i64),
        PropertyValue::Double(value) if value.is_finite() => Some(*value as i64),
        PropertyValue::Enumerated(value) => Some(i64::from(*value)),
        PropertyValue::Boolean(value) => Some(i64::from(*value)),
        _ => None,
    }
}

fn property_text(value: &PropertyValue) -> String {
    match value {
        PropertyValue::Null => String::new(),
        PropertyValue::Boolean(value) => value.to_string(),
        PropertyValue::Unsigned(value) => value.to_string(),
        PropertyValue::Signed(value) => value.to_string(),
        PropertyValue::Real(value) => value.to_string(),
        PropertyValue::Double(value) => value.to_string(),
        PropertyValue::CharacterString(value) => value.clone(),
        PropertyValue::OctetString(bytes) => bytes
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(""),
        PropertyValue::Enumerated(value) => value.to_string(),
        PropertyValue::ObjectIdentifier(value) => format!(
            "{}:{}",
            value.object_type().to_raw(),
            value.instance_number()
        ),
        other => format!("{other:?}"),
    }
}

fn default_value(value_type: TelemetryType) -> TelemetryValue {
    match value_type {
        TelemetryType::Float => TelemetryValue::Float(0.0),
        TelemetryType::Integer => TelemetryValue::Integer(0),
        TelemetryType::Boolean => TelemetryValue::Boolean(false),
        TelemetryType::Text => TelemetryValue::Text(String::new()),
    }
}
