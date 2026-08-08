use edge_core::{
    CommandFlowConfig, CommandGraphEdge, CommandGraphNode, CommandGraphNodeKind, DeviceInstance,
    DiscoveryRequest, EdgeConfigPackage, MqttUplinkConfig, OpcUaBrowsePathAddress,
    OpcUaBrowsePathElement, OpcUaConnectionSettings, OpcUaPointOptions, OpcUaWriteDataType,
    PointAccess, PointAddress, ProtocolConnection, TelemetryPointMapping, TelemetryType,
    TelemetryValue,
};
use edge_runtime::{
    CommandExecutionStatus, CommandWriteVerification, ConfiguredEdgeRuntime, OpcUaAdapter,
    ProtocolAdapter, ProtocolCommandAdapter, TokioSerialBusFactory,
};
use opcua::{
    server::{
        address_space::VariableBuilder,
        diagnostics::NamespaceMetadata,
        node_manager::memory::{simple_node_manager, SimpleNodeManager},
        ServerBuilder, ServerHandle,
    },
    types::{DataTypeId, NodeId, QualifiedName},
};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::time::{sleep, Duration, Instant};

async fn start_server() -> (
    String,
    ServerHandle,
    tokio::task::JoinHandle<Result<(), String>>,
    TempDir,
) {
    let pki = tempfile::tempdir().expect("server PKI directory");
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("OPC UA listener");
    let address = listener.local_addr().expect("listener address");
    let endpoint = format!("opc.tcp://127.0.0.1:{}/", address.port());
    let (server, handle) = ServerBuilder::new_anonymous("VelaEdge OPC UA integration server")
        .application_uri("urn:velaedge:test:opcua-server")
        .product_uri("urn:velaedge:test")
        .host("127.0.0.1")
        .pki_dir(pki.path())
        .create_sample_keypair(true)
        .discovery_urls(vec![endpoint.clone()])
        .build()
        .expect("OPC UA server builds");
    let task = tokio::spawn(server.run_with(listener));
    (endpoint, handle, task, pki)
}

async fn start_writable_server() -> (
    String,
    String,
    ServerHandle,
    tokio::task::JoinHandle<Result<(), String>>,
    TempDir,
) {
    const MANAGER_NAME: &str = "velaedge-command-test";
    const NAMESPACE_URI: &str = "urn:velaedge:test:commands";
    let pki = tempfile::tempdir().expect("server PKI directory");
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("OPC UA listener");
    let address = listener.local_addr().expect("listener address");
    let endpoint = format!("opc.tcp://127.0.0.1:{}/", address.port());
    let (server, handle) = ServerBuilder::new_anonymous("VelaEdge OPC UA writable server")
        .application_uri("urn:velaedge:test:opcua-writable-server")
        .product_uri("urn:velaedge:test")
        .host("127.0.0.1")
        .pki_dir(pki.path())
        .create_sample_keypair(true)
        .discovery_urls(vec![endpoint.clone()])
        .with_node_manager(simple_node_manager(
            NamespaceMetadata {
                namespace_uri: NAMESPACE_URI.to_string(),
                ..Default::default()
            },
            MANAGER_NAME,
        ))
        .build()
        .expect("OPC UA writable server builds");
    let manager = handle
        .node_managers()
        .get_by_name::<SimpleNodeManager>(MANAGER_NAME)
        .expect("custom command node manager");
    let namespace_index = manager
        .namespaces()
        .iter()
        .find_map(|(index, uri)| (uri == NAMESPACE_URI).then_some(*index))
        .expect("command namespace index");
    let node_id = NodeId::new(namespace_index, "Pump/SpeedSetpoint");
    assert!(
        VariableBuilder::new(
            &node_id,
            QualifiedName::new(namespace_index, "SpeedSetpoint"),
            "Speed setpoint",
        )
        .data_type(DataTypeId::UInt16)
        .value(0_u16)
        .writable()
        .insert(&mut *manager.address_space().write()),
        "writable variable is inserted"
    );
    let task = tokio::spawn(server.run_with(listener));
    (endpoint, node_id.to_string(), handle, task, pki)
}

fn connection(endpoint: &str, pki_dir: &std::path::Path) -> ProtocolConnection {
    ProtocolConnection::opc_ua(
        "opcua-main",
        endpoint,
        OpcUaConnectionSettings {
            pki_dir: pki_dir.display().to_string(),
            trust_server_certs: true,
            verify_server_certs: false,
            session_retry_limit: 1,
            ..Default::default()
        },
    )
}

fn mapping() -> TelemetryPointMapping {
    TelemetryPointMapping::new(
        "server_time",
        "opcua-server",
        "server.current_time",
        "opcua-main",
        PointAddress::opc_ua_node_id("i=2258"),
        TelemetryType::Text,
    )
}

fn service_level_mapping() -> TelemetryPointMapping {
    TelemetryPointMapping::new(
        "service_level",
        "opcua-server",
        "server.service_level",
        "opcua-main",
        PointAddress::opc_ua_node_id("i=2267"),
        TelemetryType::Integer,
    )
    .with_interval_ms(50)
}

fn service_level_browse_path_mapping() -> TelemetryPointMapping {
    TelemetryPointMapping::new(
        "service_level_path",
        "opcua-server",
        "server.service_level.path",
        "opcua-main",
        PointAddress::opc_ua_browse_path(&OpcUaBrowsePathAddress::new(
            "i=2253",
            vec![OpcUaBrowsePathElement::new(0, "ServiceLevel")],
        ))
        .expect("BrowsePath address serializes"),
        TelemetryType::Integer,
    )
    .with_interval_ms(50)
}

#[tokio::test]
async fn adapter_reads_from_a_real_opc_ua_server_and_reuses_the_session() {
    let (endpoint, server_handle, server, server_pki) = start_server().await;
    let client_pki = tempfile::tempdir().expect("client PKI directory");
    let mut adapter = OpcUaAdapter::new(connection(&endpoint, client_pki.path()), vec![mapping()])
        .expect("adapter config");

    let first = adapter.read_telemetry().await.expect("first OPC UA read");
    let second = adapter.read_telemetry().await.expect("second OPC UA read");

    assert_eq!(first.len(), 1);
    assert!(matches!(first[0].value, TelemetryValue::Text(ref value) if !value.is_empty()));
    assert_eq!(
        first[0].quality_code,
        Some(edge_core::DataQualityCode::Good)
    );
    assert_eq!(second.len(), 1);
    assert_eq!(adapter.connection_generation(), 1);

    drop(adapter);
    server_handle.cancel();
    server
        .await
        .expect("server task joins")
        .expect("server stops");
    drop(server_pki);
}

#[tokio::test]
async fn adapter_writes_and_reads_back_a_real_uint16_opc_ua_variable() {
    let (endpoint, node_id, server_handle, server, server_pki) = start_writable_server().await;
    let client_pki = tempfile::tempdir().expect("client PKI directory");
    let command_mapping = TelemetryPointMapping::new(
        "speed_setpoint",
        "pump-1",
        "pump.speed_setpoint",
        "opcua-main",
        PointAddress::opc_ua_node_id(node_id),
        TelemetryType::Integer,
    )
    .with_access(PointAccess::ReadWrite)
    .with_opc_ua_options(OpcUaPointOptions::new(OpcUaWriteDataType::UInt16));
    let mut adapter = OpcUaAdapter::new(
        connection(&endpoint, client_pki.path()),
        vec![command_mapping.clone()],
    )
    .expect("adapter config");

    let result = adapter
        .write_point(&command_mapping, TelemetryValue::Integer(1_234))
        .await
        .expect("real OPC UA Write succeeds");
    let samples = adapter
        .read_telemetry()
        .await
        .expect("written value is read back");

    assert_eq!(result.point_id, "speed_setpoint");
    assert!(
        !result.verified,
        "generic adapter leaves verification to command policy"
    );
    assert!(matches!(samples[0].value, TelemetryValue::Integer(1_234)));
    assert_eq!(adapter.connection_generation(), 1);

    drop(adapter);
    server_handle.cancel();
    server
        .await
        .expect("server task joins")
        .expect("server stops");
    drop(server_pki);
}

#[tokio::test]
async fn configured_runtime_executes_an_opc_ua_command_flow_with_readback() {
    let (endpoint, node_id, server_handle, server, server_pki) = start_writable_server().await;
    let client_pki = tempfile::tempdir().expect("client PKI directory");
    let command_mapping = TelemetryPointMapping::new(
        "speed_setpoint",
        "pump-1",
        "pump.speed_setpoint",
        "opcua-main",
        PointAddress::opc_ua_node_id(node_id),
        TelemetryType::Integer,
    )
    .with_access(PointAccess::ReadWrite)
    .with_opc_ua_options(OpcUaPointOptions::new(OpcUaWriteDataType::UInt16));
    let package = EdgeConfigPackage::new("edge-opcua-command", "v1")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(connection(&endpoint, client_pki.path()))
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://127.0.0.1:1883",
            "opcua-command-test",
        ))
        .with_point_mapping(command_mapping)
        .with_command_flow(opc_ua_write_flow());
    let mut runtime = ConfiguredEdgeRuntime::new(package, TokioSerialBusFactory)
        .expect("runtime accepts the OPC UA command package");

    let report = runtime
        .execute_command_flow_message(
            "set-speed",
            br#"{"commandId":"cmd-opcua-readback","value":2345}"#,
        )
        .await
        .expect("command flow executes");

    assert_eq!(report.status, CommandExecutionStatus::Succeeded);
    assert_eq!(report.writes.len(), 1);
    assert!(report.writes[0].verified);
    assert_eq!(
        report.writes[0].verification,
        CommandWriteVerification::Readback
    );
    assert_eq!(
        report.writes[0].readback_value,
        Some(TelemetryValue::Integer(2345))
    );
    assert_eq!(
        runtime
            .shadow("pump-1")
            .and_then(|shadow| shadow.latest_value("speed_setpoint")),
        Some(&TelemetryValue::Integer(2345))
    );
    let metrics = runtime.protocol_runtime_metrics();
    assert!(metrics[0].connected);
    assert_eq!(metrics[0].error_count, 0);

    drop(runtime);
    server_handle.cancel();
    server
        .await
        .expect("server task joins")
        .expect("server stops");
    drop(server_pki);
}

fn opc_ua_write_flow() -> CommandFlowConfig {
    let mut write = CommandGraphNode::new(
        "write-speed",
        CommandGraphNodeKind::PointWrite,
        "写入速度设定值",
    )
    .with_ref("speed_setpoint");
    write
        .params
        .insert("verification".to_string(), serde_json::json!("readback"));
    CommandFlowConfig::new(
        "set-speed",
        "设置速度",
        "velamq-main",
        "factory/{edge_id}/command/speed",
        "factory/{edge_id}/reply/{command_id}",
    )
    .with_node(CommandGraphNode::new(
        "input",
        CommandGraphNodeKind::MqttInput,
        "MQTT 输入",
    ))
    .with_node(write)
    .with_node(CommandGraphNode::new(
        "reply",
        CommandGraphNodeKind::MqttReply,
        "MQTT 回执",
    ))
    .with_edge(CommandGraphEdge::new("input-write", "input", "write-speed"))
    .with_edge(CommandGraphEdge::new("write-reply", "write-speed", "reply"))
}

#[tokio::test]
async fn adapter_browses_real_server_variables_and_infers_scalar_types() {
    let (endpoint, server_handle, server, server_pki) = start_server().await;
    let client_pki = tempfile::tempdir().expect("client PKI directory");
    let mut adapter = OpcUaAdapter::new(connection(&endpoint, client_pki.path()), Vec::new())
        .expect("adapter config");
    let request = DiscoveryRequest::opc_ua_browse("browse-server-node", "opcua-main", "i=2253", 2)
        .including_standard_namespace(true);

    let report = adapter
        .discover_variables(&request)
        .await
        .expect("OPC UA Browse discovery succeeds");

    let current_time = report
        .discovered_points
        .iter()
        .find(|point| point.address.value == "i=2258")
        .unwrap_or_else(|| {
            panic!(
                "Server CurrentTime is discovered; received {:?}",
                report.discovered_points
            )
        });
    assert_eq!(current_time.address.kind, "node_id");
    assert_eq!(current_time.value_type, TelemetryType::Text);
    assert!(!current_time.sample_values[0].is_empty());
    assert_eq!(adapter.connection_generation(), 1);

    drop(adapter);
    server_handle.cancel();
    server
        .await
        .expect("server task joins")
        .expect("server stops");
    drop(server_pki);
}

#[tokio::test]
async fn adapter_receives_real_subscription_changes_without_reconnecting() {
    let (endpoint, server_handle, server, server_pki) = start_server().await;
    let client_pki = tempfile::tempdir().expect("client PKI directory");
    let mut adapter = OpcUaAdapter::new(
        connection(&endpoint, client_pki.path()),
        vec![service_level_mapping()],
    )
    .expect("adapter config");

    adapter
        .read_telemetry()
        .await
        .expect("initial read establishes subscription");
    let deadline = Instant::now() + Duration::from_secs(2);
    while adapter.subscription_notification_count() == 0 && Instant::now() < deadline {
        sleep(Duration::from_millis(25)).await;
        adapter
            .read_telemetry()
            .await
            .expect("initial subscription notification is consumed");
    }
    assert_eq!(adapter.subscription_generation(), 1);
    assert_eq!(adapter.subscription_cached_value_count(), 1);
    let initial_notifications = adapter.subscription_notification_count();

    server_handle.set_service_level(42);
    let deadline = Instant::now() + Duration::from_secs(2);
    let updated = loop {
        sleep(Duration::from_millis(25)).await;
        let samples = adapter
            .read_telemetry()
            .await
            .expect("subscription remains readable");
        if matches!(samples[0].value, TelemetryValue::Integer(42)) {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
    };

    assert!(
        updated,
        "service-level subscription did not deliver value 42"
    );
    assert!(adapter.subscription_notification_count() > initial_notifications);
    assert_eq!(adapter.connection_generation(), 1);
    assert_eq!(adapter.subscription_generation(), 1);

    drop(adapter);
    server_handle.cancel();
    server
        .await
        .expect("server task joins")
        .expect("server stops");
    drop(server_pki);
}

#[tokio::test]
async fn adapter_translates_semantic_browse_path_once_and_subscribes_to_target() {
    let (endpoint, server_handle, server, server_pki) = start_server().await;
    let client_pki = tempfile::tempdir().expect("client PKI directory");
    let mut adapter = OpcUaAdapter::new(
        connection(&endpoint, client_pki.path()),
        vec![service_level_browse_path_mapping()],
    )
    .expect("adapter config");

    let first = adapter
        .read_telemetry()
        .await
        .expect("BrowsePath is translated and read");
    let second = adapter
        .read_telemetry()
        .await
        .expect("resolved BrowsePath is reused");

    assert_eq!(first.len(), 1);
    assert!(matches!(first[0].value, TelemetryValue::Integer(_)));
    assert_eq!(second.len(), 1);
    assert_eq!(adapter.browse_path_translation_generation(), 1);
    assert_eq!(adapter.connection_generation(), 1);
    assert_eq!(adapter.subscription_generation(), 1);

    drop(adapter);
    server_handle.cancel();
    server
        .await
        .expect("server task joins")
        .expect("server stops");
    drop(server_pki);
}

#[tokio::test]
async fn adapter_rejects_stale_subscription_cache_after_server_disconnects() {
    let (endpoint, server_handle, server, server_pki) = start_server().await;
    let client_pki = tempfile::tempdir().expect("client PKI directory");
    let mut disconnected_connection = connection(&endpoint, client_pki.path());
    let settings = disconnected_connection
        .opc_ua
        .as_mut()
        .expect("OPC UA settings");
    settings.request_timeout_ms = 250;
    settings.connect_timeout_ms = 500;
    let mut adapter = OpcUaAdapter::new(disconnected_connection, vec![service_level_mapping()])
        .expect("adapter config");

    adapter
        .read_telemetry()
        .await
        .expect("initial read establishes subscription");
    let deadline = Instant::now() + Duration::from_secs(2);
    while adapter.subscription_cached_value_count() == 0 && Instant::now() < deadline {
        sleep(Duration::from_millis(25)).await;
        adapter
            .read_telemetry()
            .await
            .expect("initial subscription notification is consumed");
    }
    assert_eq!(adapter.subscription_cached_value_count(), 1);

    server_handle.cancel();
    server
        .await
        .expect("server task joins")
        .expect("server stops");
    sleep(Duration::from_millis(1_100)).await;

    let error = adapter
        .read_telemetry()
        .await
        .expect_err("disconnected OPC UA session must not serve cached Good data");
    assert!(
        error.to_string().contains("OPC UA"),
        "unexpected disconnect error: {error:#}"
    );
    assert_eq!(adapter.subscription_cached_value_count(), 0);

    drop(adapter);
    drop(server_pki);
}

#[tokio::test]
async fn configured_runtime_executes_opc_ua_collection_branch() {
    let (endpoint, server_handle, server, server_pki) = start_server().await;
    let client_pki = tempfile::tempdir().expect("client PKI directory");
    let package = EdgeConfigPackage::new("edge-opcua", "v1")
        .with_device(DeviceInstance::new("opcua-server", "opcua"))
        .with_protocol_connection(connection(&endpoint, client_pki.path()))
        .with_point_mapping(mapping());
    let mut runtime = ConfiguredEdgeRuntime::new(package, TokioSerialBusFactory)
        .expect("runtime accepts OPC UA config");

    let report = runtime
        .collect_once()
        .await
        .expect("runtime collects OPC UA point");

    assert_eq!(report.samples_collected, 1);
    assert!(matches!(
        runtime
            .shadow("opcua-server")
            .and_then(|shadow| shadow.latest_value("server_time")),
        Some(TelemetryValue::Text(value)) if !value.is_empty()
    ));
    let metrics = runtime.protocol_runtime_metrics();
    assert!(metrics[0].connected);
    assert_eq!(metrics[0].good_value_count, 1);

    drop(runtime);
    server_handle.cancel();
    server
        .await
        .expect("server task joins")
        .expect("server stops");
    drop(server_pki);
}
