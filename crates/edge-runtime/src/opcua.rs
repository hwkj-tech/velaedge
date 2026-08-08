use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use edge_core::{
    parse_opc_ua_browse_path, DataQuality, DataQualityCode, DiscoveredPoint, DiscoveryAddressKind,
    DiscoveryReport, DiscoveryRequest, OpcUaAuthMode, OpcUaMessageSecurityMode, OpcUaWriteDataType,
    PointAddress, ProtocolConnection, ProtocolType, TelemetryPointMapping, TelemetrySample,
    TelemetryType, TelemetryValue, MAX_DISCOVERY_POINTS,
};
use opcua::{
    client::{ClientBuilder, DataChangeCallback, IdentityToken, Session},
    types::{
        AttributeId, BrowseDescription, BrowseDirection, BrowsePath, BrowseResultMask, DataValue,
        EndpointDescription, MessageSecurityMode, MonitoredItemCreateRequest, MonitoringMode,
        MonitoringParameters, NodeClass, NodeClassMask, NodeId, NumericRange, QualifiedName,
        ReadValueId, ReferenceDescription, ReferenceTypeId, RelativePath, RelativePathElement,
        StatusCode, TimestampsToReturn, Variant, WriteValue,
    },
};
use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedReceiver},
    task::JoinHandle,
    time::timeout,
};

use crate::{ProtocolAdapter, ProtocolCommandAdapter, ProtocolWriteResult};

const MAX_OPC_UA_BROWSE_NODES: usize = 512;
const MAX_OPC_UA_REFERENCES_PER_NODE: usize = 512;
const MAX_OPC_UA_NOTIFICATIONS_PER_PUBLISH: u32 = 1_024;
const OPC_UA_SUBSCRIPTION_LIFETIME_COUNT: u32 = 100;
const OPC_UA_SUBSCRIPTION_KEEP_ALIVE_COUNT: u32 = 10;

pub struct OpcUaAdapter {
    connection: ProtocolConnection,
    mappings: Vec<TelemetryPointMapping>,
    session: Option<Arc<Session>>,
    event_loop: Option<JoinHandle<StatusCode>>,
    connection_generation: u64,
    subscription: Option<OpcUaSubscription>,
    subscription_generation: u64,
    resolved_nodes: Option<ResolvedOpcUaNodes>,
    browse_path_translation_generation: u64,
}

impl OpcUaAdapter {
    pub fn new(
        connection: ProtocolConnection,
        mappings: Vec<TelemetryPointMapping>,
    ) -> Result<Self> {
        if connection.protocol != ProtocolType::OpcUa {
            bail!("OPC UA adapter requires an OPC UA connection");
        }
        connection.validate().map_err(anyhow::Error::msg)?;
        validate_mappings(&connection, &mappings)?;
        Ok(Self {
            connection,
            mappings,
            session: None,
            event_loop: None,
            connection_generation: 0,
            subscription: None,
            subscription_generation: 0,
            resolved_nodes: None,
            browse_path_translation_generation: 0,
        })
    }

    pub fn set_mappings(&mut self, mappings: Vec<TelemetryPointMapping>) -> Result<()> {
        validate_mappings(&self.connection, &mappings)?;
        self.mappings = mappings;
        Ok(())
    }

    pub fn connection_generation(&self) -> u64 {
        self.connection_generation
    }

    pub fn subscription_generation(&self) -> u64 {
        self.subscription_generation
    }

    pub fn subscription_notification_count(&self) -> u64 {
        self.subscription
            .as_ref()
            .map_or(0, |subscription| subscription.notification_count)
    }

    pub fn subscription_cached_value_count(&self) -> usize {
        self.subscription
            .as_ref()
            .map_or(0, |subscription| subscription.latest_values.len())
    }

    pub fn browse_path_translation_generation(&self) -> u64 {
        self.browse_path_translation_generation
    }

    pub async fn discover_variables(
        &mut self,
        request: &DiscoveryRequest,
    ) -> Result<DiscoveryReport> {
        request
            .validate()
            .map_err(anyhow::Error::msg)
            .context("invalid OPC UA discovery request")?;
        if request.address_kind != DiscoveryAddressKind::OpcUaBrowse {
            bail!("OPC UA discovery requires an opc_ua_browse request");
        }
        if request.protocol_connection_id != self.connection.connection_id {
            bail!("discovery request targets a different protocol connection");
        }
        let root_node_id = NodeId::from_str(
            request
                .root_node_id
                .as_deref()
                .expect("validated OPC UA discovery root NodeId"),
        )
        .context("invalid OPC UA discovery root NodeId")?;
        let session = self.session().await?;
        let request_timeout = Duration::from_millis(
            self.connection
                .opc_ua
                .as_ref()
                .expect("validated OPC UA settings")
                .request_timeout_ms,
        );
        let discovery = timeout(
            request_timeout,
            discover_variable_nodes(
                &session,
                root_node_id,
                request.max_depth,
                request.include_standard_namespace,
            ),
        )
        .await;
        let discovered_points = match discovery {
            Ok(Ok(points)) => points,
            Ok(Err(error)) => {
                self.clear_session();
                return Err(error.context("OPC UA Browse discovery failed"));
            }
            Err(_) => {
                self.clear_session();
                bail!("OPC UA Browse discovery timed out");
            }
        };

        Ok(DiscoveryReport {
            job_id: request.job_id.clone(),
            protocol_connection_id: request.protocol_connection_id.clone(),
            discovered_points: discovered_points
                .into_iter()
                .map(|point| {
                    DiscoveredPoint::new(
                        &request.protocol_connection_id,
                        PointAddress::opc_ua_node_id(point.node_id.to_string()),
                        point.value_type,
                    )
                    .with_sample_values(vec![point.sample_value])
                    .with_confidence(point.confidence)
                })
                .collect(),
            suggestions: Vec::new(),
        })
    }

    async fn session(&mut self) -> Result<Arc<Session>> {
        if self
            .event_loop
            .as_ref()
            .is_some_and(JoinHandle::is_finished)
        {
            self.clear_session();
        }
        if let Some(session) = &self.session {
            return Ok(session.clone());
        }

        let settings = self
            .connection
            .opc_ua
            .as_ref()
            .context("OPC UA settings are required")?;
        let endpoint_url = self
            .connection
            .endpoint
            .as_deref()
            .context("OPC UA endpoint is required")?;
        let identity = identity_token(settings)?;
        let endpoint: EndpointDescription = (
            endpoint_url,
            settings.security_policy.as_str(),
            message_security_mode(settings.message_security_mode),
        )
            .into();
        let mut client = ClientBuilder::new()
            .application_name("VelaEdge Runtime")
            .application_uri(format!(
                "urn:velaedge:runtime:{}",
                self.connection.connection_id
            ))
            .product_uri("https://github.com/hwkj-tech/velaedge")
            .pki_dir(&settings.pki_dir)
            .create_sample_keypair(true)
            .trust_server_certs(settings.trust_server_certs)
            .verify_server_certs(settings.verify_server_certs)
            .session_retry_limit(settings.session_retry_limit as i32)
            .session_timeout(settings.session_timeout_ms)
            .client()
            .map_err(|errors| {
                anyhow!("invalid OPC UA client configuration: {}", errors.join("; "))
            })?;

        let connect_timeout = Duration::from_millis(settings.connect_timeout_ms);
        let (session, event_loop) = timeout(
            connect_timeout,
            client.connect_to_matching_endpoint(endpoint, identity),
        )
        .await
        .context("OPC UA endpoint discovery timed out")?
        .context("OPC UA endpoint discovery failed")?;
        let event_loop = event_loop.spawn();
        let connected = timeout(connect_timeout, session.wait_for_connection())
            .await
            .context("OPC UA session activation timed out")?;
        if !connected {
            event_loop.abort();
            bail!("OPC UA session event loop stopped before activation");
        }

        self.connection_generation = self.connection_generation.saturating_add(1);
        self.session = Some(session.clone());
        self.event_loop = Some(event_loop);
        Ok(session)
    }

    fn clear_session(&mut self) {
        if let Some(event_loop) = self.event_loop.take() {
            event_loop.abort();
        }
        self.subscription = None;
        self.resolved_nodes = None;
        self.session = None;
    }

    async fn resolve_read_nodes(&mut self, session: &Session) -> Result<Vec<ReadValueId>> {
        let signature = address_resolution_signature(&self.mappings)?;
        if let Some(resolved) = &self.resolved_nodes {
            if resolved.signature == signature {
                return Ok(resolved.nodes.clone());
            }
        }

        let mut nodes = vec![None; self.mappings.len()];
        let mut browse_paths = Vec::new();
        let mut browse_path_indexes = Vec::new();
        for (index, mapping) in self.mappings.iter().enumerate() {
            match mapping.address.kind.as_str() {
                "node_id" => nodes[index] = Some(node_id_read_value_id(mapping)?),
                "browse_path" => {
                    let path = parse_opc_ua_browse_path(&mapping.address.value)
                        .map_err(anyhow::Error::msg)
                        .with_context(|| {
                            format!("invalid OPC UA BrowsePath for point {}", mapping.point_id)
                        })?;
                    browse_paths.push(BrowsePath {
                        starting_node: NodeId::from_str(&path.starting_node)
                            .context("invalid OPC UA BrowsePath starting NodeId")?,
                        relative_path: RelativePath {
                            elements: Some(
                                path.elements
                                    .into_iter()
                                    .map(|element| RelativePathElement {
                                        reference_type_id: ReferenceTypeId::HierarchicalReferences
                                            .into(),
                                        is_inverse: false,
                                        include_subtypes: true,
                                        target_name: QualifiedName::new(
                                            element.namespace_index,
                                            element.target_name,
                                        ),
                                    })
                                    .collect(),
                            ),
                        },
                    });
                    browse_path_indexes.push(index);
                }
                _ => unreachable!("OPC UA mapping address kind was validated"),
            }
        }

        if !browse_paths.is_empty() {
            let translated = match timeout(
                self.request_timeout(),
                session.translate_browse_paths_to_node_ids(&browse_paths),
            )
            .await
            {
                Ok(Ok(results)) => results,
                Ok(Err(error)) => {
                    self.clear_session();
                    return Err(anyhow!(error)
                        .context("OPC UA TranslateBrowsePathsToNodeIds service failed"));
                }
                Err(_) => {
                    self.clear_session();
                    bail!("OPC UA TranslateBrowsePathsToNodeIds timed out");
                }
            };
            if translated.len() != browse_path_indexes.len() {
                bail!(
                    "OPC UA BrowsePath result count mismatch: requested {}, received {}",
                    browse_path_indexes.len(),
                    translated.len()
                );
            }
            for (mapping_index, result) in browse_path_indexes.into_iter().zip(translated) {
                let mapping = &self.mappings[mapping_index];
                if result.status_code.is_bad() {
                    bail!(
                        "OPC UA BrowsePath for point {} failed with {}",
                        mapping.point_id,
                        result.status_code
                    );
                }
                let targets = result.targets.unwrap_or_default();
                let mut complete_targets = targets.into_iter().filter(|target| {
                    target.remaining_path_index == u32::MAX
                        && target.target_id.server_index == 0
                        && target.target_id.namespace_uri.is_empty()
                });
                let target = complete_targets.next().with_context(|| {
                    format!(
                        "OPC UA BrowsePath for point {} resolved no local complete target",
                        mapping.point_id
                    )
                })?;
                if complete_targets.next().is_some() {
                    bail!(
                        "OPC UA BrowsePath for point {} is ambiguous",
                        mapping.point_id
                    );
                }
                nodes[mapping_index] = Some(ReadValueId {
                    node_id: target.target_id.node_id,
                    attribute_id: AttributeId::Value as u32,
                    ..Default::default()
                });
            }
            self.browse_path_translation_generation =
                self.browse_path_translation_generation.saturating_add(1);
        }

        let nodes = nodes
            .into_iter()
            .enumerate()
            .map(|(index, node)| {
                node.with_context(|| {
                    format!(
                        "OPC UA address for point {} was not resolved",
                        self.mappings[index].point_id
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.resolved_nodes = Some(ResolvedOpcUaNodes {
            signature,
            nodes: nodes.clone(),
        });
        Ok(nodes)
    }

    async fn ensure_subscription(
        &mut self,
        session: &Session,
        nodes: &[ReadValueId],
    ) -> Result<()> {
        let signature = subscription_signature(&self.mappings, nodes)?;
        if signature.is_empty() {
            self.subscription = None;
            return Ok(());
        }
        if self.subscription.as_ref().is_some_and(|subscription| {
            subscription.signature == signature && !subscription.receiver.is_closed()
        }) {
            return Ok(());
        }

        let request_timeout = self.request_timeout();
        if let Some(subscription) = self.subscription.take() {
            let _ = timeout(
                request_timeout,
                session.delete_subscription(subscription.id),
            )
            .await;
        }

        let (sender, receiver) = unbounded_channel();
        let publishing_interval = Duration::from_millis(
            signature
                .iter()
                .map(|item| item.sampling_interval_ms)
                .min()
                .unwrap_or(1_000),
        );
        let subscription_id = timeout(
            request_timeout,
            session.create_subscription(
                publishing_interval,
                OPC_UA_SUBSCRIPTION_LIFETIME_COUNT,
                OPC_UA_SUBSCRIPTION_KEEP_ALIVE_COUNT,
                MAX_OPC_UA_NOTIFICATIONS_PER_PUBLISH,
                0,
                true,
                DataChangeCallback::new(move |value, monitored_item| {
                    let node_id = monitored_item.item_to_monitor().node_id.to_string();
                    let _ = sender.send((node_id, value));
                }),
            ),
        )
        .await
        .context("OPC UA CreateSubscription timed out")?
        .map_err(anyhow::Error::from)
        .context("OPC UA CreateSubscription failed")?;
        let items = signature
            .iter()
            .map(|item| {
                Ok(MonitoredItemCreateRequest {
                    item_to_monitor: ReadValueId {
                        node_id: NodeId::from_str(&item.node_id)
                            .context("invalid OPC UA subscription NodeId")?,
                        attribute_id: AttributeId::Value as u32,
                        ..Default::default()
                    },
                    monitoring_mode: MonitoringMode::Reporting,
                    requested_parameters: MonitoringParameters {
                        sampling_interval: item.sampling_interval_ms as f64,
                        queue_size: 1,
                        discard_oldest: true,
                        ..Default::default()
                    },
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let created = match timeout(
            request_timeout,
            session.create_monitored_items(subscription_id, TimestampsToReturn::Both, items),
        )
        .await
        {
            Ok(Ok(created)) => created,
            Ok(Err(error)) => {
                let _ = session.delete_subscription(subscription_id).await;
                return Err(anyhow!(error).context("OPC UA CreateMonitoredItems failed"));
            }
            Err(_) => {
                let _ = session.delete_subscription(subscription_id).await;
                bail!("OPC UA CreateMonitoredItems timed out");
            }
        };
        if let Some(rejected) = created.iter().find(|item| item.result.status_code.is_bad()) {
            let status = rejected.result.status_code;
            let _ = session.delete_subscription(subscription_id).await;
            bail!("OPC UA monitored item was rejected with {status}");
        }

        self.subscription = Some(OpcUaSubscription {
            id: subscription_id,
            signature,
            receiver,
            latest_values: BTreeMap::new(),
            notification_count: 0,
            health_check_interval: subscription_health_check_interval(publishing_interval),
            last_health_check_at: Instant::now(),
        });
        self.subscription_generation = self.subscription_generation.saturating_add(1);
        Ok(())
    }

    fn drain_subscription_notifications(&mut self) {
        let Some(subscription) = self.subscription.as_mut() else {
            return;
        };
        while let Ok((node_id, value)) = subscription.receiver.try_recv() {
            subscription.latest_values.insert(node_id, value);
            subscription.notification_count = subscription.notification_count.saturating_add(1);
        }
    }

    async fn read_values(
        &mut self,
        session: &Session,
        nodes: &[ReadValueId],
    ) -> Result<Vec<DataValue>> {
        match timeout(
            self.request_timeout(),
            session.read(nodes, TimestampsToReturn::Both, 0.0),
        )
        .await
        {
            Ok(Ok(values)) => Ok(values),
            Ok(Err(error)) => {
                self.clear_session();
                Err(anyhow!(error).context("OPC UA read service failed"))
            }
            Err(_) => {
                self.clear_session();
                bail!("OPC UA read service timed out")
            }
        }
    }

    fn request_timeout(&self) -> Duration {
        Duration::from_millis(
            self.connection
                .opc_ua
                .as_ref()
                .expect("validated OPC UA settings")
                .request_timeout_ms,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OpcUaSubscriptionItem {
    node_id: String,
    sampling_interval_ms: u64,
}

struct OpcUaSubscription {
    id: u32,
    signature: Vec<OpcUaSubscriptionItem>,
    receiver: UnboundedReceiver<(String, DataValue)>,
    latest_values: BTreeMap<String, DataValue>,
    notification_count: u64,
    health_check_interval: Duration,
    last_health_check_at: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OpcUaAddressSignature {
    kind: String,
    value: String,
}

struct ResolvedOpcUaNodes {
    signature: Vec<OpcUaAddressSignature>,
    nodes: Vec<ReadValueId>,
}

struct BrowsedVariable {
    node_id: NodeId,
    value_type: TelemetryType,
    sample_value: String,
    confidence: f64,
}

async fn discover_variable_nodes(
    session: &Session,
    root_node_id: NodeId,
    max_depth: u8,
    include_standard_namespace: bool,
) -> Result<Vec<BrowsedVariable>> {
    let mut queue = VecDeque::from([(root_node_id, 0_u8)]);
    let mut visited = HashSet::new();
    let mut variables = Vec::new();
    let mut variable_ids = HashSet::new();
    let limit = usize::from(MAX_DISCOVERY_POINTS);

    while let Some((node_id, depth)) = queue.pop_front() {
        if visited.len() >= MAX_OPC_UA_BROWSE_NODES
            || !visited.insert(node_id.clone())
            || depth >= max_depth
            || variables.len() >= limit
        {
            continue;
        }
        for reference in
            browse_hierarchical_references(session, node_id, MAX_OPC_UA_REFERENCES_PER_NODE).await?
        {
            if reference.node_id.server_index != 0 || !reference.node_id.namespace_uri.is_empty() {
                continue;
            }
            let child = reference.node_id.node_id;
            if !include_standard_namespace && child.namespace == 0 {
                continue;
            }
            let child_depth = depth.saturating_add(1);
            match reference.node_class {
                NodeClass::Object
                    if child_depth < max_depth
                        && visited.len().saturating_add(queue.len()) < MAX_OPC_UA_BROWSE_NODES =>
                {
                    queue.push_back((child, child_depth));
                }
                NodeClass::Variable => {
                    if variables.len() < limit && variable_ids.insert(child.clone()) {
                        variables.push(child.clone());
                    }
                    if child_depth < max_depth
                        && visited.len().saturating_add(queue.len()) < MAX_OPC_UA_BROWSE_NODES
                    {
                        queue.push_back((child, child_depth));
                    }
                }
                _ => {}
            }
        }
    }

    if variables.is_empty() {
        return Ok(Vec::new());
    }
    let read_nodes = variables
        .iter()
        .cloned()
        .map(|node_id| ReadValueId {
            node_id,
            attribute_id: AttributeId::Value as u32,
            ..Default::default()
        })
        .collect::<Vec<_>>();
    let values = session
        .read(&read_nodes, TimestampsToReturn::Both, 0.0)
        .await
        .map_err(anyhow::Error::from)
        .context("read discovered OPC UA variables")?;

    Ok(variables
        .into_iter()
        .zip(values)
        .filter_map(|(node_id, data_value)| {
            let status = data_value.status.unwrap_or(StatusCode::Good);
            if status.is_bad() {
                return None;
            }
            let value = data_value.value?;
            let value_type = inferred_telemetry_type(&value)?;
            Some(BrowsedVariable {
                node_id,
                value_type,
                sample_value: variant_text(&value),
                confidence: if status.is_uncertain() { 0.7 } else { 0.9 },
            })
        })
        .collect())
}

async fn browse_hierarchical_references(
    session: &Session,
    node_id: NodeId,
    reference_limit: usize,
) -> Result<Vec<ReferenceDescription>> {
    let description = BrowseDescription {
        node_id,
        browse_direction: BrowseDirection::Forward,
        reference_type_id: ReferenceTypeId::HierarchicalReferences.into(),
        include_subtypes: true,
        node_class_mask: (NodeClassMask::OBJECT | NodeClassMask::VARIABLE).bits(),
        result_mask: BrowseResultMask::All as u32,
    };
    let mut results = session
        .browse(&[description], 64, None)
        .await
        .map_err(anyhow::Error::from)
        .context("browse OPC UA node")?;
    let mut result = results
        .pop()
        .context("OPC UA Browse returned no result for requested node")?;
    if result.status_code.is_bad() {
        bail!("OPC UA Browse failed with {}", result.status_code);
    }
    let mut references = result
        .references
        .take()
        .unwrap_or_default()
        .into_iter()
        .take(reference_limit)
        .collect::<Vec<_>>();
    while !result.continuation_point.is_null() {
        let continuation_point = result.continuation_point.clone();
        if references.len() >= reference_limit {
            session
                .browse_next(true, &[continuation_point])
                .await
                .map_err(anyhow::Error::from)
                .context("release bounded OPC UA Browse continuation point")?;
            break;
        }
        let mut next_results = session
            .browse_next(false, &[continuation_point])
            .await
            .map_err(anyhow::Error::from)
            .context("continue OPC UA Browse")?;
        result = next_results
            .pop()
            .context("OPC UA BrowseNext returned no result")?;
        if result.status_code.is_bad() {
            bail!("OPC UA BrowseNext failed with {}", result.status_code);
        }
        references.extend(
            result
                .references
                .take()
                .unwrap_or_default()
                .into_iter()
                .take(reference_limit.saturating_sub(references.len())),
        );
    }
    Ok(references)
}

fn inferred_telemetry_type(value: &Variant) -> Option<TelemetryType> {
    match value {
        Variant::Boolean(_) => Some(TelemetryType::Boolean),
        Variant::SByte(_)
        | Variant::Byte(_)
        | Variant::Int16(_)
        | Variant::UInt16(_)
        | Variant::Int32(_)
        | Variant::UInt32(_)
        | Variant::Int64(_)
        | Variant::UInt64(_) => Some(TelemetryType::Integer),
        Variant::Float(_) | Variant::Double(_) => Some(TelemetryType::Float),
        Variant::String(_) | Variant::DateTime(_) => Some(TelemetryType::Text),
        _ => None,
    }
}

impl Drop for OpcUaAdapter {
    fn drop(&mut self) {
        self.clear_session();
    }
}

#[async_trait]
impl ProtocolAdapter for OpcUaAdapter {
    async fn read_telemetry(&mut self) -> Result<Vec<TelemetrySample>> {
        if self.mappings.is_empty() {
            return Ok(Vec::new());
        }
        let session = self.session().await?;
        let nodes = self.resolve_read_nodes(&session).await?;
        let subscription_ready = self.ensure_subscription(&session, &nodes).await.is_ok();
        if subscription_ready {
            self.drain_subscription_notifications();
        }
        let cached_values = self.subscription.as_ref().map(|subscription| {
            nodes
                .iter()
                .map(|node| {
                    subscription
                        .latest_values
                        .get(&node.node_id.to_string())
                        .cloned()
                })
                .collect::<Vec<_>>()
        });
        let cache_complete = cached_values
            .as_ref()
            .is_some_and(|values| values.iter().all(Option::is_some));
        let health_check_due = self.subscription.as_ref().is_some_and(|subscription| {
            subscription.last_health_check_at.elapsed() >= subscription.health_check_interval
        });
        let values: Vec<DataValue> = if cache_complete && !health_check_due {
            cached_values
                .expect("checked subscription cache")
                .into_iter()
                .map(|value| value.expect("checked subscription cache entry"))
                .collect()
        } else {
            let fallback = self.read_values(&session, &nodes).await?;
            if subscription_ready {
                if let Some(subscription) = self.subscription.as_mut() {
                    subscription.last_health_check_at = Instant::now();
                }
            }
            cached_values
                .unwrap_or_else(|| vec![None; fallback.len()])
                .into_iter()
                .zip(fallback)
                .map(|(cached, fallback)| cached.unwrap_or(fallback))
                .collect()
        };
        if values.len() != self.mappings.len() {
            bail!(
                "OPC UA read response count mismatch: requested {}, received {}",
                self.mappings.len(),
                values.len()
            );
        }

        Ok(self
            .mappings
            .iter()
            .zip(values)
            .map(|(mapping, value)| data_value_to_sample(mapping, value))
            .collect())
    }
}

#[async_trait]
impl ProtocolCommandAdapter for OpcUaAdapter {
    async fn write_point(
        &mut self,
        mapping: &TelemetryPointMapping,
        value: TelemetryValue,
    ) -> Result<ProtocolWriteResult> {
        if mapping.protocol_connection_id != self.connection.connection_id {
            bail!(
                "OPC UA point {} references connection {} instead of {}",
                mapping.point_id,
                mapping.protocol_connection_id,
                self.connection.connection_id
            );
        }
        if !mapping.access.is_writable() {
            bail!("OPC UA point {} is not writable", mapping.point_id);
        }
        edge_core::validate_opc_ua_point(
            &mapping.address,
            mapping.value_type,
            mapping.access,
            mapping.opc_ua,
        )
        .map_err(anyhow::Error::msg)?;

        self.set_mappings(vec![mapping.clone()])?;
        let session = self.session().await?;
        let nodes = self.resolve_read_nodes(&session).await?;
        let node = nodes
            .into_iter()
            .next()
            .context("OPC UA write point did not resolve to a node")?;
        let variant = opc_ua_write_variant(mapping, &value)?;
        let write = WriteValue::new(
            node.node_id,
            AttributeId::Value,
            NumericRange::None,
            DataValue::value_only(variant),
        );
        let statuses = match timeout(self.request_timeout(), session.write(&[write])).await {
            Ok(Ok(statuses)) => statuses,
            Ok(Err(error)) => {
                self.clear_session();
                return Err(anyhow!(error).context("OPC UA Write service failed"));
            }
            Err(_) => {
                self.clear_session();
                bail!("OPC UA Write service timed out");
            }
        };
        let status = statuses
            .first()
            .copied()
            .context("OPC UA Write service returned no point status")?;
        if status.is_bad() {
            bail!(
                "OPC UA server rejected write for point {} with status {status:?}",
                mapping.point_id
            );
        }

        Ok(ProtocolWriteResult {
            point_id: mapping.point_id.clone(),
            value,
            verified: false,
            readback_value: None,
        })
    }
}

fn opc_ua_write_variant(
    mapping: &TelemetryPointMapping,
    value: &TelemetryValue,
) -> Result<Variant> {
    let data_type = mapping
        .opc_ua
        .context("writable OPC UA point requires writeDataType")?
        .write_data_type;
    let conversion_error = || {
        anyhow!(
            "OPC UA point {} value {value:?} cannot be encoded as {data_type:?}",
            mapping.point_id
        )
    };
    Ok(match (data_type, value) {
        (OpcUaWriteDataType::Boolean, TelemetryValue::Boolean(value)) => Variant::Boolean(*value),
        (OpcUaWriteDataType::SByte, TelemetryValue::Integer(value)) => {
            Variant::SByte(i8::try_from(*value).map_err(|_| conversion_error())?)
        }
        (OpcUaWriteDataType::Byte, TelemetryValue::Integer(value)) => {
            Variant::Byte(u8::try_from(*value).map_err(|_| conversion_error())?)
        }
        (OpcUaWriteDataType::Int16, TelemetryValue::Integer(value)) => {
            Variant::Int16(i16::try_from(*value).map_err(|_| conversion_error())?)
        }
        (OpcUaWriteDataType::UInt16, TelemetryValue::Integer(value)) => {
            Variant::UInt16(u16::try_from(*value).map_err(|_| conversion_error())?)
        }
        (OpcUaWriteDataType::Int32, TelemetryValue::Integer(value)) => {
            Variant::Int32(i32::try_from(*value).map_err(|_| conversion_error())?)
        }
        (OpcUaWriteDataType::UInt32, TelemetryValue::Integer(value)) => {
            Variant::UInt32(u32::try_from(*value).map_err(|_| conversion_error())?)
        }
        (OpcUaWriteDataType::Int64, TelemetryValue::Integer(value)) => Variant::Int64(*value),
        (OpcUaWriteDataType::UInt64, TelemetryValue::Integer(value)) => {
            Variant::UInt64(u64::try_from(*value).map_err(|_| conversion_error())?)
        }
        (OpcUaWriteDataType::Float, TelemetryValue::Float(value))
            if value.is_finite() && value.abs() <= f64::from(f32::MAX) =>
        {
            Variant::Float(*value as f32)
        }
        (OpcUaWriteDataType::Double, TelemetryValue::Float(value)) if value.is_finite() => {
            Variant::Double(*value)
        }
        (OpcUaWriteDataType::String, TelemetryValue::Text(value)) => {
            Variant::String(value.clone().into())
        }
        _ => return Err(conversion_error()),
    })
}

fn subscription_signature(
    mappings: &[TelemetryPointMapping],
    read_nodes: &[ReadValueId],
) -> Result<Vec<OpcUaSubscriptionItem>> {
    if mappings.len() != read_nodes.len() {
        bail!("OPC UA mapping and resolved node counts differ");
    }
    let mut nodes = BTreeMap::<String, u64>::new();
    for (mapping, read_node) in mappings.iter().zip(read_nodes) {
        let node_id = read_node.node_id.to_string();
        let sampling_interval_ms = mapping.interval_ms.max(1);
        nodes
            .entry(node_id)
            .and_modify(|current| *current = (*current).min(sampling_interval_ms))
            .or_insert(sampling_interval_ms);
    }
    Ok(nodes
        .into_iter()
        .map(|(node_id, sampling_interval_ms)| OpcUaSubscriptionItem {
            node_id,
            sampling_interval_ms,
        })
        .collect())
}

fn address_resolution_signature(
    mappings: &[TelemetryPointMapping],
) -> Result<Vec<OpcUaAddressSignature>> {
    mappings
        .iter()
        .map(|mapping| {
            validate_mapping_address(mapping)?;
            Ok(OpcUaAddressSignature {
                kind: mapping.address.kind.clone(),
                value: mapping.address.value.clone(),
            })
        })
        .collect()
}

fn subscription_health_check_interval(publishing_interval: Duration) -> Duration {
    const MIN_HEALTH_CHECK_INTERVAL_MS: u128 = 1_000;
    const MAX_HEALTH_CHECK_INTERVAL_MS: u128 = 60_000;

    let keep_alive_window_ms = publishing_interval
        .as_millis()
        .saturating_mul(u128::from(OPC_UA_SUBSCRIPTION_KEEP_ALIVE_COUNT));
    Duration::from_millis(
        keep_alive_window_ms
            .clamp(MIN_HEALTH_CHECK_INTERVAL_MS, MAX_HEALTH_CHECK_INTERVAL_MS)
            .try_into()
            .expect("health-check interval is bounded to u64"),
    )
}

fn identity_token(settings: &edge_core::OpcUaConnectionSettings) -> Result<IdentityToken> {
    match settings.auth_mode {
        OpcUaAuthMode::Anonymous => Ok(IdentityToken::Anonymous),
        OpcUaAuthMode::Username => {
            let username = settings
                .username
                .as_deref()
                .context("OPC UA username is required")?;
            let password_env = settings
                .password_env
                .as_deref()
                .context("OPC UA password environment variable is required")?;
            let password = std::env::var(password_env).with_context(|| {
                format!("OPC UA password environment variable {password_env} is not set")
            })?;
            Ok(IdentityToken::new_user_name(username, password))
        }
        OpcUaAuthMode::X509 => IdentityToken::new_x509_path(
            settings
                .user_certificate_path
                .as_deref()
                .context("OPC UA user certificate path is required")?,
            settings
                .user_private_key_path
                .as_deref()
                .context("OPC UA user private key path is required")?,
        )
        .context("failed to load OPC UA X.509 identity"),
    }
}

fn message_security_mode(mode: OpcUaMessageSecurityMode) -> MessageSecurityMode {
    match mode {
        OpcUaMessageSecurityMode::None => MessageSecurityMode::None,
        OpcUaMessageSecurityMode::Sign => MessageSecurityMode::Sign,
        OpcUaMessageSecurityMode::SignAndEncrypt => MessageSecurityMode::SignAndEncrypt,
    }
}

fn validate_mappings(
    connection: &ProtocolConnection,
    mappings: &[TelemetryPointMapping],
) -> Result<()> {
    for mapping in mappings {
        if mapping.protocol_connection_id != connection.connection_id {
            bail!(
                "OPC UA point {} references connection {} instead of {}",
                mapping.point_id,
                mapping.protocol_connection_id,
                connection.connection_id
            );
        }
        validate_mapping_address(mapping)?;
    }
    Ok(())
}

fn validate_mapping_address(mapping: &TelemetryPointMapping) -> Result<()> {
    match mapping.address.kind.as_str() {
        "node_id" => {
            NodeId::from_str(&mapping.address.value).with_context(|| {
                format!(
                    "invalid OPC UA NodeId {} for point {}",
                    mapping.address.value, mapping.point_id
                )
            })?;
        }
        "browse_path" => {
            parse_opc_ua_browse_path(&mapping.address.value)
                .map_err(anyhow::Error::msg)
                .with_context(|| {
                    format!("invalid OPC UA BrowsePath for point {}", mapping.point_id)
                })?;
        }
        _ => bail!(
            "OPC UA point {} requires node_id or browse_path address kind",
            mapping.point_id
        ),
    }
    Ok(())
}

fn node_id_read_value_id(mapping: &TelemetryPointMapping) -> Result<ReadValueId> {
    let node_id = NodeId::from_str(&mapping.address.value).with_context(|| {
        format!(
            "invalid OPC UA NodeId {} for point {}",
            mapping.address.value, mapping.point_id
        )
    })?;
    Ok(ReadValueId {
        node_id,
        attribute_id: AttributeId::Value as u32,
        ..Default::default()
    })
}

fn data_value_to_sample(mapping: &TelemetryPointMapping, data_value: DataValue) -> TelemetrySample {
    let status = data_value.status.unwrap_or(StatusCode::Good);
    let timestamp = data_value
        .source_timestamp
        .or(data_value.server_timestamp)
        .map(|timestamp| timestamp.as_chrono())
        .unwrap_or_else(Utc::now);
    let (value, decode_failed) = match data_value.value {
        Some(value) => match telemetry_value(&value, mapping.value_type) {
            Ok(value) => (value, false),
            Err(_) => (default_value(mapping.value_type), true),
        },
        None => (default_value(mapping.value_type), true),
    };
    let quality_code = if decode_failed {
        DataQualityCode::BadDecode
    } else if status.is_bad() {
        DataQualityCode::BadProtocol
    } else if status.is_uncertain() {
        DataQualityCode::UncertainProtocol
    } else {
        DataQualityCode::Good
    };
    TelemetrySample::new(
        &mapping.device_id,
        &mapping.point_id,
        value,
        DataQuality::Good,
        timestamp,
    )
    .with_quality_code(quality_code)
}

fn telemetry_value(value: &Variant, value_type: TelemetryType) -> Result<TelemetryValue> {
    match value_type {
        TelemetryType::Float => variant_f64(value)
            .map(TelemetryValue::Float)
            .context("OPC UA value is not numeric"),
        TelemetryType::Integer => variant_i64(value)
            .map(TelemetryValue::Integer)
            .context("OPC UA value is not an integer or exceeds i64"),
        TelemetryType::Boolean => match value {
            Variant::Boolean(value) => Ok(TelemetryValue::Boolean(*value)),
            _ => bail!("OPC UA value is not boolean"),
        },
        TelemetryType::Text => Ok(TelemetryValue::Text(variant_text(value))),
    }
}

fn variant_f64(value: &Variant) -> Option<f64> {
    match value {
        Variant::SByte(value) => Some(f64::from(*value)),
        Variant::Byte(value) => Some(f64::from(*value)),
        Variant::Int16(value) => Some(f64::from(*value)),
        Variant::UInt16(value) => Some(f64::from(*value)),
        Variant::Int32(value) => Some(f64::from(*value)),
        Variant::UInt32(value) => Some(f64::from(*value)),
        Variant::Int64(value) => Some(*value as f64),
        Variant::UInt64(value) => Some(*value as f64),
        Variant::Float(value) => Some(f64::from(*value)),
        Variant::Double(value) => Some(*value),
        _ => None,
    }
    .filter(|value| value.is_finite())
}

fn variant_i64(value: &Variant) -> Option<i64> {
    match value {
        Variant::SByte(value) => Some(i64::from(*value)),
        Variant::Byte(value) => Some(i64::from(*value)),
        Variant::Int16(value) => Some(i64::from(*value)),
        Variant::UInt16(value) => Some(i64::from(*value)),
        Variant::Int32(value) => Some(i64::from(*value)),
        Variant::UInt32(value) => Some(i64::from(*value)),
        Variant::Int64(value) => Some(*value),
        Variant::UInt64(value) => i64::try_from(*value).ok(),
        _ => None,
    }
}

fn variant_text(value: &Variant) -> String {
    match value {
        Variant::String(value) => value.to_string(),
        Variant::Boolean(value) => value.to_string(),
        Variant::SByte(value) => value.to_string(),
        Variant::Byte(value) => value.to_string(),
        Variant::Int16(value) => value.to_string(),
        Variant::UInt16(value) => value.to_string(),
        Variant::Int32(value) => value.to_string(),
        Variant::UInt32(value) => value.to_string(),
        Variant::Int64(value) => value.to_string(),
        Variant::UInt64(value) => value.to_string(),
        Variant::Float(value) => value.to_string(),
        Variant::Double(value) => value.to_string(),
        Variant::DateTime(value) => value.as_chrono().to_rfc3339(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_variant_scalars_to_requested_telemetry_type() {
        assert_eq!(
            telemetry_value(&Variant::UInt16(42), TelemetryType::Float).unwrap(),
            TelemetryValue::Float(42.0)
        );
        assert_eq!(
            telemetry_value(&Variant::Int32(-7), TelemetryType::Integer).unwrap(),
            TelemetryValue::Integer(-7)
        );
        assert_eq!(
            telemetry_value(&Variant::Boolean(true), TelemetryType::Boolean).unwrap(),
            TelemetryValue::Boolean(true)
        );
    }

    #[test]
    fn rejects_scalar_type_mismatch() {
        assert!(telemetry_value(&Variant::Boolean(true), TelemetryType::Float).is_err());
        assert!(telemetry_value(&Variant::Double(1.5), TelemetryType::Integer).is_err());
    }
}
