use std::collections::BTreeSet;

use chrono::Utc;
use cloud_control::{
    AgentAuthorizationRequest, AgentChangeOperation, AgentChangeSet, AgentChangeSetError,
    AgentChangeSetStatus, AgentChangeTarget, AgentCommandCandidate, AgentCommandCandidateError,
    AgentCommandCandidateStatus, AgentCommandTarget, AgentConfirmationEvidence,
    AgentConfirmationPolicy, AgentImpactSummary, AgentPermission, AgentResourceKind,
    AgentRiskLevel, AgentToolCaller, AgentToolCategory, AgentToolDecision, AgentToolDescriptor,
    AgentToolEffect, AgentToolExposure, AgentToolRegistry, AgentToolRegistryError,
    AgentValidationIssue, AgentValidationReport, AgentValidationSeverity,
};
use serde_json::json;

fn permissions(values: &[AgentPermission]) -> BTreeSet<AgentPermission> {
    values.iter().copied().collect()
}

fn human_confirmation(note: Option<&str>) -> AgentConfirmationEvidence {
    AgentConfirmationEvidence {
        confirmed_by: "operator@example.com".to_owned(),
        co_approver: None,
        note: note.map(str::to_owned),
        confirmed_at: Utc::now(),
    }
}

#[test]
fn read_tools_are_model_callable_without_confirmation() {
    let registry = AgentToolRegistry::velaedge_v2();
    let request = AgentAuthorizationRequest {
        caller: AgentToolCaller::Model,
        permissions: permissions(&[AgentPermission::RuntimeRead]),
        confirmation: None,
    };

    assert_eq!(
        registry.authorize("runtime.metrics", &request).unwrap(),
        AgentToolDecision::Allowed
    );
    assert!(registry
        .model_tools()
        .all(|tool| !tool.effect.mutates_state()));
}

#[test]
fn operational_diagnosis_requires_all_live_evidence_permissions() {
    let registry = AgentToolRegistry::velaedge_v2();
    let partial = AgentAuthorizationRequest {
        caller: AgentToolCaller::Model,
        permissions: permissions(&[AgentPermission::RuntimeRead]),
        confirmation: None,
    };
    assert!(matches!(
        registry.authorize("operations.diagnose", &partial).unwrap(),
        AgentToolDecision::Denied { .. }
    ));

    let complete = AgentAuthorizationRequest {
        caller: AgentToolCaller::Model,
        permissions: permissions(&[
            AgentPermission::RuntimeRead,
            AgentPermission::ConfigurationRead,
            AgentPermission::MqttRead,
        ]),
        confirmation: None,
    };
    assert_eq!(
        registry
            .authorize("operations.diagnose", &complete)
            .unwrap(),
        AgentToolDecision::Allowed
    );
}

#[test]
fn registry_rejects_model_callable_mutation_tools() {
    let mut registry = AgentToolRegistry::default();
    let error = registry
        .register(AgentToolDescriptor {
            name: "unsafe.apply".to_owned(),
            description: "Unsafe mutation".to_owned(),
            category: AgentToolCategory::Configuration,
            effect: AgentToolEffect::ApplyChangeSet,
            exposure: AgentToolExposure::ModelCallable,
            risk: AgentRiskLevel::High,
            confirmation: AgentConfirmationPolicy::HumanUser,
            required_permissions: permissions(&[AgentPermission::ConfigurationApply]),
            input_schema: json!({ "type": "object" }),
            output_schema: json!({ "type": "object" }),
        })
        .unwrap_err();

    assert_eq!(
        error,
        AgentToolRegistryError::ModelMutationForbidden("unsafe.apply".to_owned())
    );
}

#[test]
fn configuration_apply_is_control_plane_only_and_requires_confirmation() {
    let registry = AgentToolRegistry::velaedge_v2();
    let model_request = AgentAuthorizationRequest {
        caller: AgentToolCaller::Model,
        permissions: permissions(&[AgentPermission::ConfigurationApply]),
        confirmation: Some(human_confirmation(Some("approved"))),
    };
    assert!(matches!(
        registry
            .authorize("configuration.change_set.apply", &model_request)
            .unwrap(),
        AgentToolDecision::Denied { .. }
    ));

    let control_plane_request = AgentAuthorizationRequest {
        caller: AgentToolCaller::ControlPlane,
        permissions: permissions(&[AgentPermission::ConfigurationApply]),
        confirmation: None,
    };
    assert_eq!(
        registry
            .authorize("configuration.change_set.apply", &control_plane_request)
            .unwrap(),
        AgentToolDecision::ConfirmationRequired {
            policy: AgentConfirmationPolicy::HumanUser
        }
    );

    let confirmed_request = AgentAuthorizationRequest {
        confirmation: Some(human_confirmation(Some("approved"))),
        ..control_plane_request
    };
    assert_eq!(
        registry
            .authorize("configuration.change_set.apply", &confirmed_request)
            .unwrap(),
        AgentToolDecision::Allowed
    );
}

#[test]
fn device_dispatch_requires_governed_control_plane_execution() {
    let registry = AgentToolRegistry::velaedge_v2();
    let request = AgentAuthorizationRequest {
        caller: AgentToolCaller::ControlPlane,
        permissions: permissions(&[AgentPermission::CommandDispatch]),
        confirmation: None,
    };

    assert_eq!(
        registry
            .authorize("device.command.dispatch", &request)
            .unwrap(),
        AgentToolDecision::ConfirmationRequired {
            policy: AgentConfirmationPolicy::PrivilegedReviewer
        }
    );
    assert_eq!(
        registry.get("device.command.dispatch").unwrap().exposure,
        AgentToolExposure::ControlPlaneOnly
    );
}

#[test]
fn change_set_must_validate_and_receive_human_confirmation_before_apply() {
    let operation = AgentChangeOperation::update(
        AgentResourceKind::CollectionFlow,
        "flow-main",
        json!({ "intervalMs": 1000 }),
        json!({ "intervalMs": 500 }),
    );
    let mut change_set = AgentChangeSet::new(
        "Increase collection frequency",
        "Reduce latency for the pressure telemetry path",
        AgentChangeTarget {
            project_id: "plant-a".to_owned(),
            product_id: Some("pump-product".to_owned()),
            product_version: Some("v2.2.0".to_owned()),
            edge_ids: ["edge-1".to_owned()].into_iter().collect(),
        },
        "revision-41",
        vec![operation],
        AgentRiskLevel::High,
        "agent:model-gateway",
    )
    .unwrap();

    assert!(matches!(
        change_set.start_apply(),
        Err(AgentChangeSetError::InvalidTransition { .. })
    ));

    change_set
        .record_validation(AgentValidationReport::new(
            Vec::new(),
            AgentImpactSummary {
                affected_resources: 1,
                affected_edges: ["edge-1".to_owned()].into_iter().collect(),
                requires_runtime_sync: true,
                command_path_changed: false,
                notes: vec!["runtime configuration will be synchronized".to_owned()],
            },
        ))
        .unwrap();
    assert_eq!(
        change_set.status,
        AgentChangeSetStatus::AwaitingConfirmation
    );

    let model_confirmation = AgentConfirmationEvidence {
        confirmed_by: "agent:model-gateway".to_owned(),
        co_approver: None,
        note: Some("self approved".to_owned()),
        confirmed_at: Utc::now(),
    };
    assert_eq!(
        change_set.confirm(model_confirmation).unwrap_err(),
        AgentChangeSetError::HumanConfirmationRequired
    );
    assert_eq!(
        change_set.confirm(human_confirmation(None)).unwrap_err(),
        AgentChangeSetError::ReviewNoteRequired
    );

    change_set
        .confirm(human_confirmation(Some(
            "Reviewed the 500 ms device and broker capacity impact",
        )))
        .unwrap();
    change_set.start_apply().unwrap();
    change_set
        .mark_applied(json!({ "revision": "revision-42", "runtimeSynced": true }))
        .unwrap();

    assert_eq!(change_set.status, AgentChangeSetStatus::Applied);
    assert_eq!(change_set.result.as_ref().unwrap()["runtimeSynced"], true);
}

#[test]
fn validation_errors_keep_change_set_in_draft() {
    let operation = AgentChangeOperation::create(
        AgentResourceKind::ProtocolConnection,
        "unsafe-connection",
        json!({ "protocol": "modbus_tcp" }),
    );
    let mut change_set = AgentChangeSet::new(
        "Create connection",
        "Generated candidate",
        AgentChangeTarget {
            project_id: "plant-a".to_owned(),
            product_id: Some("pump-product".to_owned()),
            product_version: None,
            edge_ids: BTreeSet::new(),
        },
        "revision-41",
        vec![operation],
        AgentRiskLevel::Medium,
        "agent:model-gateway",
    )
    .unwrap();

    change_set
        .record_validation(AgentValidationReport::new(
            vec![AgentValidationIssue {
                severity: AgentValidationSeverity::Error,
                code: "protocol.host.required".to_owned(),
                path: "operations[0].after.host".to_owned(),
                message: "Modbus TCP host is required".to_owned(),
            }],
            AgentImpactSummary::default(),
        ))
        .unwrap();

    assert_eq!(change_set.status, AgentChangeSetStatus::Draft);
    assert_eq!(
        change_set
            .confirm(human_confirmation(Some("approve")))
            .unwrap_err(),
        AgentChangeSetError::InvalidTransition {
            from: AgentChangeSetStatus::Draft,
            action: "confirm"
        }
    );
}

fn command_candidate(risk: AgentRiskLevel) -> AgentCommandCandidate {
    AgentCommandCandidate::new(
        "Start pump",
        "Operator requested a controlled pump start",
        AgentCommandTarget {
            project_id: "plant-a".to_owned(),
            edge_id: "edge-1".to_owned(),
            product_id: Some("pump-product".to_owned()),
            product_version: Some("v2.2.0".to_owned()),
            flow_id: "pump-command".to_owned(),
            protocol_connection_id: "modbus-line".to_owned(),
            device_id: "pump-1".to_owned(),
            point_id: "pump-start".to_owned(),
        },
        json!(true),
        "operator-42-start-pump-1",
        risk,
        "agent:model-gateway",
    )
    .unwrap()
}

#[test]
fn command_candidate_requires_validation_and_human_confirmation_before_dispatch() {
    let mut candidate = command_candidate(AgentRiskLevel::High);
    assert!(matches!(
        candidate.start_dispatch(),
        Err(AgentCommandCandidateError::InvalidTransition { .. })
    ));

    candidate
        .record_validation(AgentValidationReport::new(
            Vec::new(),
            AgentImpactSummary {
                affected_resources: 1,
                affected_edges: ["edge-1".to_owned()].into_iter().collect(),
                requires_runtime_sync: false,
                command_path_changed: true,
                notes: vec!["writes one governed point".to_owned()],
            },
        ))
        .unwrap();
    assert_eq!(
        candidate.status,
        AgentCommandCandidateStatus::AwaitingConfirmation
    );
    assert_eq!(
        candidate
            .confirm(AgentConfirmationEvidence {
                confirmed_by: "agent:model-gateway".to_owned(),
                co_approver: None,
                note: Some("self approved".to_owned()),
                confirmed_at: Utc::now(),
            })
            .unwrap_err(),
        AgentCommandCandidateError::HumanConfirmationRequired
    );

    candidate
        .confirm(human_confirmation(Some(
            "Verified writable point and target device",
        )))
        .unwrap();
    candidate.start_dispatch().unwrap();
    candidate
        .mark_dispatched(json!({"brokerAcknowledged": true}))
        .unwrap();
    assert_eq!(candidate.status, AgentCommandCandidateStatus::Dispatched);
    assert!(matches!(
        candidate.start_dispatch(),
        Err(AgentCommandCandidateError::InvalidTransition { .. })
    ));
}

#[test]
fn critical_command_candidate_requires_two_distinct_human_approvers() {
    let mut candidate = command_candidate(AgentRiskLevel::Critical);
    candidate
        .record_validation(AgentValidationReport::new(
            Vec::new(),
            AgentImpactSummary::default(),
        ))
        .unwrap();
    assert_eq!(
        candidate
            .confirm(human_confirmation(Some("Primary safety review complete")))
            .unwrap_err(),
        AgentCommandCandidateError::TwoPersonConfirmationRequired
    );
    candidate
        .confirm(AgentConfirmationEvidence {
            confirmed_by: "operator@example.com".to_owned(),
            co_approver: Some("safety@example.com".to_owned()),
            note: Some("Independent safety review complete".to_owned()),
            confirmed_at: Utc::now(),
        })
        .unwrap();
    assert_eq!(candidate.status, AgentCommandCandidateStatus::Confirmed);
}
