use chrono::{TimeZone, Utc};
use edge_core::{DataQuality, TelemetrySample, TelemetryValue};
use edge_runtime::{EdgeRuntime, JsonlLocalStore, ProtocolAdapter, SimulatedProtocolAdapter};
use tempfile::tempdir;

fn pressure_sample() -> TelemetrySample {
    TelemetrySample::new(
        "pump-1",
        "pressure",
        TelemetryValue::Float(2.4),
        DataQuality::Good,
        Utc.with_ymd_and_hms(2026, 6, 26, 8, 30, 0).unwrap(),
    )
}

#[tokio::test]
async fn simulated_protocol_adapter_returns_configured_samples() {
    let expected = pressure_sample();
    let mut adapter = SimulatedProtocolAdapter::new(vec![expected.clone()]);

    let samples = adapter.read_telemetry().await.unwrap();

    assert_eq!(samples, vec![expected]);
}

#[tokio::test]
async fn collect_once_updates_shadow_and_persists_jsonl() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("telemetry.jsonl");
    let adapter = SimulatedProtocolAdapter::new(vec![pressure_sample()]);
    let store = JsonlLocalStore::new(&path);
    let mut runtime = EdgeRuntime::new("edge-1", "pump-1", adapter, store);

    let report = runtime.collect_once().await.unwrap();

    assert_eq!(report.samples_collected, 1);
    assert_eq!(
        runtime.shadow().latest_value("pressure"),
        Some(&TelemetryValue::Float(2.4))
    );

    let persisted = tokio::fs::read_to_string(path).await.unwrap();
    let lines = persisted.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("\"telemetry_id\":\"pressure\""));
}
