use cloud_control::{
    AgentCommandDraft, AgentProposal, AgentProposalKind, AgentProposalReviewError,
    AgentProposalRisk, AgentProposalStatus, ConfigPackage, EdgeNode, FleetRegistry,
};
use edge_core::{
    AlgorithmDsl, AlgorithmInputBinding, AlgorithmKind, AlgorithmOutput, AlgorithmReportMode,
    AlgorithmReportPolicy, AlgorithmSpec, AlgorithmStep, AlgorithmTrigger, CommandParameter,
    CommandRisk, CommandSpec, DeviceSpec, NumberRange, PolicyEngine, TelemetryType, TelemetryValue,
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
fn agent_proposal_separates_author_and_reviewer() {
    let mut proposal = AgentProposal::new(
        "fleet-agent",
        AgentProposalKind::ConfigSuggestion,
        "调整采集周期",
        "建议将压力点采集周期调整为 2 秒",
        "operator-a",
    );

    assert_eq!(
        proposal.review(AgentProposalStatus::Approved, "operator-a", None),
        Err(AgentProposalReviewError::SelfReview)
    );
    assert_eq!(proposal.status, AgentProposalStatus::PendingReview);
}

#[test]
fn high_risk_agent_proposal_requires_approval_note() {
    let mut proposal = AgentProposal::new(
        "fleet-agent",
        AgentProposalKind::CommandCandidate,
        "调整泵速",
        "建议调整生产泵运行速度",
        "operator-a",
    );
    proposal.risk = AgentProposalRisk::High;

    assert_eq!(
        proposal.review(AgentProposalStatus::Approved, "reviewer-a", None),
        Err(AgentProposalReviewError::ApprovalNoteRequired)
    );
    proposal
        .review(
            AgentProposalStatus::Approved,
            "reviewer-a",
            Some("已核对联锁条件，仅允许进入人工配置流程".to_string()),
        )
        .unwrap();
    assert_eq!(proposal.status, AgentProposalStatus::Approved);
}

#[test]
fn config_package_targets_edge_and_versions_algorithms() {
    let algorithm = AlgorithmSpec::dsl(
        "pump-anomaly",
        "1.0.0",
        AlgorithmKind::ChangeReport,
        AlgorithmDsl {
            inputs: vec![AlgorithmInputBinding::new("pressure", "pressure")],
            trigger: AlgorithmTrigger::on_sample(),
            steps: vec![AlgorithmStep::change_filter("pressure", 0.05)],
            outputs: vec![AlgorithmOutput::virtual_point("pressure", "anomaly_score")],
            report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnChange, "velamq-main"),
        },
    );
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

#[test]
fn agent_proposal_requires_one_terminal_human_review() {
    let mut proposal = AgentProposal::new(
        "fleet-agent",
        AgentProposalKind::ConfigSuggestion,
        "补全压力点位",
        "建议增加 pump.pressure 映射",
        "operator-a",
    );

    proposal
        .review(
            AgentProposalStatus::Approved,
            "reviewer-a",
            Some("仅批准进入人工配置流程".to_string()),
        )
        .unwrap();

    assert_eq!(proposal.status, AgentProposalStatus::Approved);
    assert_eq!(proposal.reviewed_by.as_deref(), Some("reviewer-a"));
    assert!(proposal.reviewed_at.is_some());
    assert!(proposal
        .review(AgentProposalStatus::Rejected, "reviewer-b", None)
        .is_err());
}
