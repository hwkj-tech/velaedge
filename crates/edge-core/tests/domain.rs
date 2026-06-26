use std::collections::BTreeMap;

use chrono::Utc;
use edge_core::{
    CloudEnvelope, CommandCandidate, CommandParameter, CommandRisk, CommandSpec, DataQuality,
    DeviceShadow, DeviceSpec, NumberRange, PolicyEngine, TelemetryPoint, TelemetrySample,
    TelemetryType, TelemetryValue,
};

#[test]
fn device_spec_finds_telemetry_metadata_by_id() {
    let spec = DeviceSpec::new("pump", "1.0.0").with_telemetry(vec![
        TelemetryPoint::new("pressure", TelemetryType::Float)
            .with_unit("MPa")
            .with_range(NumberRange::new(0.0, 20.0)),
        TelemetryPoint::new("running", TelemetryType::Boolean),
    ]);

    let pressure = spec.telemetry("pressure").expect("pressure point exists");

    assert_eq!(pressure.id, "pressure");
    assert_eq!(pressure.unit.as_deref(), Some("MPa"));
    assert_eq!(pressure.range, Some(NumberRange::new(0.0, 20.0)));
    assert!(spec.telemetry("missing").is_none());
}

#[test]
fn device_shadow_tracks_latest_sample_per_telemetry_id() {
    let mut shadow = DeviceShadow::new("edge-1", "pump-1");
    let sample = TelemetrySample::new(
        "pump-1",
        "pressure",
        TelemetryValue::Float(3.8),
        DataQuality::Good,
        Utc::now(),
    );

    shadow.update(sample);

    assert_eq!(
        shadow.latest_value("pressure"),
        Some(&TelemetryValue::Float(3.8))
    );
    assert!(shadow.latest_value("temperature").is_none());
}

#[test]
fn policy_engine_rejects_out_of_range_command_parameter() {
    let spec = DeviceSpec::new("pump", "1.0.0").with_commands(vec![CommandSpec::new(
        "set_speed",
        CommandRisk::Medium,
    )
    .with_parameter(
        CommandParameter::new("rpm", TelemetryType::Float)
            .with_range(NumberRange::new(0.0, 3_600.0)),
    )]);
    let mut parameters = BTreeMap::new();
    parameters.insert("rpm".to_string(), TelemetryValue::Float(4_000.0));
    let candidate = CommandCandidate::new("edge-1", "pump-1", "set_speed", parameters)
        .requested_by("agent:fleet-ops");

    let violation = PolicyEngine
        .validate_command(&spec, &candidate)
        .expect_err("rpm should be rejected");

    assert!(violation
        .to_string()
        .contains("parameter `rpm` value 4000 is outside range 0..=3600"));
}

#[test]
fn cloud_envelope_preserves_identity_schema_and_payload() {
    let payload = TelemetryValue::Boolean(true);
    let envelope = CloudEnvelope::new("edge-1", payload.clone());

    assert_eq!(envelope.edge_id, "edge-1");
    assert_eq!(envelope.schema_version, "1.0");
    assert_eq!(envelope.payload, payload);
    assert!(!envelope.message_id.to_string().is_empty());
}
