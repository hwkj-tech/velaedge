use cloud_control::{AgentCommandDraft, ConfigPackage, EdgeNode, FleetRegistry};
use edge_core::{
    AlgorithmDsl, AlgorithmKind, AlgorithmRuntime, AlgorithmSpec, CommandParameter, CommandRisk,
    CommandSpec, DeviceSpec, NumberRange, PolicyEngine, TelemetryType, TelemetryValue,
};

#[test]
fn fleet_registry_stores_and_retrieves_edge_nodes() {
    let mut registry = FleetRegistry::default();
    let node = EdgeNode::new("edge-1", "Shanghai Line 1").with_capability("modbus");

    registry.register(node.clone());

    assert_eq!(registry.get("edge-1"), Some(&node));
    assert!(registry.get("missing").is_none());
}

#[test]
fn config_package_targets_edge_and_versions_algorithms() {
    let algorithm = AlgorithmSpec {
        id: "pump-anomaly".to_string(),
        version: "1.0.0".to_string(),
        kind: AlgorithmKind::ChangeReport,
        dsl: AlgorithmDsl::default(),
        runtime: AlgorithmRuntime::Onnx,
        inputs: vec!["pressure".to_string()],
        outputs: vec!["anomaly_score".to_string()],
    };
    let package = ConfigPackage::new("edge-1", "2026.06.26")
        .with_algorithm(algorithm.clone())
        .with_device_spec(DeviceSpec::new("pump", "1.0.0"));

    assert_eq!(package.edge_id, "edge-1");
    assert_eq!(package.version, "2026.06.26");
    assert_eq!(package.algorithms, vec![algorithm]);
    assert_eq!(package.device_specs[0].device_type, "pump");
}

#[test]
fn agent_command_draft_converts_to_policy_checkable_candidate() {
    let spec = DeviceSpec::new("pump", "1.0.0").with_commands(vec![CommandSpec::new(
        "set_speed",
        CommandRisk::Medium,
    )
    .with_parameter(
        CommandParameter::new("rpm", TelemetryType::Float)
            .with_range(NumberRange::new(0.0, 3_600.0)),
    )]);
    let candidate = AgentCommandDraft::new("fleet-ops", "edge-1", "pump-1", "set_speed")
        .with_parameter("rpm", TelemetryValue::Float(1_200.0))
        .with_rationale("Keep discharge pressure stable during ramp-up")
        .into_candidate();

    assert_eq!(candidate.requested_by, "agent:fleet-ops");
    assert_eq!(candidate.edge_id, "edge-1");
    assert!(PolicyEngine.validate_command(&spec, &candidate).is_ok());
}
