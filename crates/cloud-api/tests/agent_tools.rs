use cloud_api::{AgentToolCall, AgentToolRuntime, AppState, CloudAgentToolRuntime};
use edge_core::{
    CommandFlowConfig, CommandGraphEdge, CommandGraphNode, CommandGraphNodeKind, EdgeRuntimeEvent,
    MqttUplinkConfig, PointAccess, ProtocolCircuitState, RuntimeEventCategory,
    RuntimeEventSeverity,
};
use serde_json::{json, Value};

fn runtime(state: &AppState) -> CloudAgentToolRuntime {
    CloudAgentToolRuntime::new(state.store.clone())
}

fn call(name: &str, arguments: Value) -> AgentToolCall {
    AgentToolCall {
        call_id: format!("call-{name}"),
        name: name.to_owned(),
        arguments,
    }
}

fn scoped_context(project_id: Option<&str>, edge_id: Option<&str>) -> Value {
    json!({
        "scope": {
            "projectId": project_id,
            "edgeId": edge_id,
        }
    })
}

#[test]
fn exposes_structured_read_tools_for_all_operational_resources() {
    let state = AppState::default();
    let names = runtime(&state)
        .descriptors()
        .into_iter()
        .map(|descriptor| descriptor.name)
        .collect::<Vec<_>>();

    for expected in [
        "project.list",
        "product.inspect",
        "configuration.inspect",
        "protocol.inspect",
        "point_set.inspect",
        "collection_flow.inspect",
        "command_flow.inspect",
        "runtime.metrics",
        "mqtt.status",
        "operations.diagnose",
        "audit.search",
        "knowledge.search",
        "device.command.draft",
    ] {
        assert!(names.contains(&expected.to_owned()), "missing {expected}");
    }
}

fn install_governed_command_flow(state: &AppState) {
    let mut store = state.store.lock().unwrap();
    let mut package = store
        .latest_config_package_for_edge("edge-dev")
        .unwrap()
        .clone();
    package
        .point_mappings
        .iter_mut()
        .find(|mapping| mapping.point_id == "running")
        .unwrap()
        .access = PointAccess::ReadWrite;
    let mut safety = CommandGraphNode::new(
        "safety",
        CommandGraphNodeKind::SafetyGate,
        "Human-confirmed safety gate",
    );
    safety
        .params
        .insert("require_confirmation".to_owned(), json!(true));
    let mut write = CommandGraphNode::new(
        "write-running",
        CommandGraphNodeKind::PointWrite,
        "Write running",
    )
    .with_ref("running");
    write
        .params
        .insert("value_path".to_owned(), json!("values.running"));
    let mut flow = CommandFlowConfig::new(
        "pump-command",
        "Pump command",
        "velamq-main",
        "factory/edge-dev/pump-1/command",
        "factory/edge-dev/pump-1/command/reply/{command_id}",
    )
    .with_protocol_connection("modbus-line-a");
    flow.nodes = vec![
        CommandGraphNode::new("input", CommandGraphNodeKind::MqttInput, "MQTT input"),
        safety,
        write,
        CommandGraphNode::new("reply", CommandGraphNodeKind::MqttReply, "MQTT reply"),
    ];
    flow.edges = vec![
        CommandGraphEdge::new("input-safety", "input", "safety"),
        CommandGraphEdge::new("safety-write", "safety", "write-running"),
        CommandGraphEdge::new("write-reply", "write-running", "reply"),
    ];
    package.command_flows = vec![flow];
    store.upsert_config_package(package);
}

#[test]
fn command_draft_only_targets_a_valid_writable_point_and_never_dispatches() {
    let state = AppState::default();
    install_governed_command_flow(&state);
    let context = scoped_context(Some("demo-plant"), Some("edge-dev"));
    let result = runtime(&state)
        .execute(
            &call(
                "device.command.draft",
                json!({
                    "projectId": "demo-plant",
                    "edgeId": "edge-dev",
                    "flowId": "pump-command",
                    "pointId": "running",
                    "deviceId": "pump-1",
                    "value": true,
                    "title": "Start pump",
                    "rationale": "Operator requested a controlled start",
                    "idempotencyKey": "start-pump-1-001"
                }),
            ),
            &context,
        )
        .unwrap();

    assert_eq!(result["dispatched"], false);
    assert_eq!(result["commandCandidate"]["target"]["pointId"], "running");
    assert_eq!(result["commandCandidate"]["status"], "draft");

    let read_only = runtime(&state)
        .execute(
            &call(
                "device.command.draft",
                json!({
                    "projectId": "demo-plant",
                    "edgeId": "edge-dev",
                    "flowId": "pump-command",
                    "pointId": "pressure",
                    "value": 2.5,
                    "title": "Write pressure",
                    "rationale": "Must be rejected",
                    "idempotencyKey": "pressure-write-001"
                }),
            ),
            &context,
        )
        .unwrap_err();
    assert!(read_only
        .to_string()
        .contains("does not expose writable point"));
}

#[test]
fn knowledge_search_returns_scoped_chunk_citations() {
    let state = AppState::default();
    let result = runtime(&state)
        .execute(
            &call(
                "knowledge.search",
                json!({"query": "Modbus 指令写入和可写点位"}),
            ),
            &scoped_context(Some("demo-plant"), None),
        )
        .unwrap();

    assert_eq!(result["schemaVersion"], "velaedge.agent.tool/v1");
    assert!(result["count"].as_u64().is_some_and(|count| count > 0));
    assert!(result["items"][0]["chunkId"].as_str().is_some());
    assert!(result["items"][0]["contentHash"]
        .as_str()
        .is_some_and(|hash| hash.starts_with("sha256:")));
}

#[test]
fn project_scope_cannot_be_overridden_by_model_arguments() {
    let state = AppState::default();
    let runtime = runtime(&state);
    let context = scoped_context(Some("demo-plant"), None);

    let result = runtime
        .execute(&call("project.list", json!({})), &context)
        .unwrap();
    assert_eq!(result["schemaVersion"], "velaedge.agent.tool/v1");
    assert_eq!(result["count"], 1);
    assert_eq!(result["items"][0]["project"]["projectId"], "demo-plant");

    let error = runtime
        .execute(
            &call("project.list", json!({"projectId": "energy-demo"})),
            &context,
        )
        .unwrap_err();
    assert!(error.to_string().contains("outside the active Agent scope"));
}

#[test]
fn configuration_tools_return_live_protocol_point_and_flow_data() {
    let state = AppState::default();
    let runtime = runtime(&state);
    let context = scoped_context(Some("demo-plant"), Some("edge-dev"));

    let protocols = runtime
        .execute(&call("protocol.inspect", json!({})), &context)
        .unwrap();
    assert!(protocols["items"][0]["protocolConnections"]
        .as_array()
        .is_some_and(|items| !items.is_empty()));
    assert_eq!(
        protocols["items"][0]["edgeId"],
        Value::String("edge-dev".to_owned())
    );

    let collection = runtime
        .execute(&call("collection_flow.inspect", json!({})), &context)
        .unwrap();
    assert!(collection["items"][0]["collectionFlows"].is_array());

    let commands = runtime
        .execute(&call("command_flow.inspect", json!({})), &context)
        .unwrap();
    assert!(commands["items"][0]["commandFlows"].is_array());

    let point_sets = runtime
        .execute(
            &call("point_set.inspect", json!({})),
            &scoped_context(Some("demo-plant"), None),
        )
        .unwrap();
    assert!(point_sets["count"].as_u64().is_some_and(|count| count > 0));
}

#[test]
fn edge_scope_blocks_cross_edge_runtime_queries() {
    let state = AppState::default();
    let runtime = runtime(&state);
    let context = scoped_context(Some("demo-plant"), Some("edge-dev"));

    let error = runtime
        .execute(
            &call("runtime.metrics", json!({"edgeId": "other-edge"})),
            &context,
        )
        .unwrap_err();
    assert!(error.to_string().contains("outside the active Agent scope"));
}

#[test]
fn mqtt_tool_reports_capability_without_exposing_secret_references() {
    let state = AppState::default();
    let mut uplink = MqttUplinkConfig::velamq(
        "sensitive-sink",
        "mqtts://broker.example:8883",
        "edge-dev-agent-test",
    );
    uplink.username = Some("edge-user".to_owned());
    uplink.password_env = Some("TOP_SECRET_PASSWORD_ENV".to_owned());
    uplink.tls_ca_path = Some("/secret/ca.pem".to_owned());
    state
        .store
        .lock()
        .unwrap()
        .upsert_mqtt_uplink("edge-dev", uplink);

    let result = runtime(&state)
        .execute(
            &call("mqtt.status", json!({})),
            &scoped_context(Some("demo-plant"), Some("edge-dev")),
        )
        .unwrap();
    let encoded = serde_json::to_string(&result).unwrap();
    assert!(!encoded.contains("TOP_SECRET_PASSWORD_ENV"));
    assert!(!encoded.contains("/secret/ca.pem"));
    assert_eq!(result["items"][0]["configured"]["passwordConfigured"], true);
    assert_eq!(result["items"][0]["configured"]["tlsConfigured"], true);
}

#[test]
fn operational_diagnosis_correlates_protocol_mqtt_sync_and_buffer_failures() {
    let state = AppState::default();
    {
        let mut store = state.store.lock().unwrap();
        let mut snapshot = store.runtime_metrics("edge-dev").unwrap().clone();
        snapshot.cloud_sync.desired_version = "v1.4.4".to_owned();
        snapshot.cloud_sync.reported_version = "v1.4.3".to_owned();
        snapshot.protocols[0].connected = false;
        snapshot.protocols[0].collection_attempt_count = 100;
        snapshot.protocols[0].collection_success_count = 70;
        snapshot.protocols[0].consecutive_failure_count = 4;
        snapshot.protocols[0].circuit_state = ProtocolCircuitState::Open;
        let mut s7 = snapshot.protocols[0].clone();
        s7.connection_id = "s7-line-a".to_owned();
        s7.protocol = "SiemensS7".to_owned();
        s7.collection_attempt_count = 40;
        s7.collection_success_count = 39;
        s7.consecutive_failure_count = 1;
        s7.circuit_state = ProtocolCircuitState::HalfOpen;
        snapshot.protocols.push(s7);
        snapshot.mqtt.configured_sink_count = 1;
        snapshot.mqtt.connected_sink_count = 0;
        snapshot.mqtt.publish_success_count = 0;
        snapshot.mqtt.publish_failure_count = 5;
        snapshot.local_store.buffered_records = 120;
        snapshot.local_store.oldest_buffer_age_seconds = 30;
        store.upsert_runtime_metrics(snapshot);
        store.push_runtime_event(EdgeRuntimeEvent::new(
            "edge-dev",
            RuntimeEventSeverity::Warning,
            RuntimeEventCategory::Protocol,
            "modbus.timeout",
            "Modbus request timed out",
        ));
    }

    let result = runtime(&state)
        .execute(
            &call("operations.diagnose", json!({"eventLimit": 10})),
            &scoped_context(Some("demo-plant"), Some("edge-dev")),
        )
        .unwrap();
    let diagnosis = &result["items"][0];
    assert_eq!(diagnosis["overall"], "critical");
    assert_eq!(diagnosis["confidence"], "high");
    assert_eq!(diagnosis["evidenceCoverage"]["observedProtocols"], 2);
    assert!(serde_json::to_string(diagnosis)
        .unwrap()
        .contains("SiemensS7"));
    let codes = diagnosis["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|finding| finding["code"].as_str())
        .collect::<Vec<_>>();
    for expected in [
        "configuration_revision_drift",
        "protocol_disconnected",
        "protocol_circuit_not_closed",
        "protocol_collection_degraded",
        "mqtt_sink_disconnected",
        "mqtt_publish_failures",
        "collection_delivery_gap",
        "local_buffer_backlog",
        "recent_runtime_events",
    ] {
        assert!(codes.contains(&expected), "missing diagnosis {expected}");
    }
}
