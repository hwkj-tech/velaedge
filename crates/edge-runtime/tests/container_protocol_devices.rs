use std::time::Duration;

use edge_core::{
    BacnetCovSettings, BacnetIpConnectionSettings, BacnetPointOptions, Iec104ControlType,
    Iec104PointOptions, OmronFinsConnectionSettings, OmronFinsTransport, PointAccess, PointAddress,
    ProtocolConnection, SiemensS7ConnectionSettings, TelemetryPointMapping, TelemetryType,
    TelemetryValue,
};
use edge_runtime::{
    BacnetIpAdapter, Iec104Adapter, OmronFinsAdapter, ProtocolAdapter, ProtocolCommandAdapter,
    SiemensS7Adapter,
};

fn mapping(
    point_id: &str,
    connection_id: &str,
    address: PointAddress,
    value_type: TelemetryType,
    access: PointAccess,
) -> TelemetryPointMapping {
    TelemetryPointMapping::new(
        point_id,
        "container-plc",
        point_id,
        connection_id,
        address,
        value_type,
    )
    .with_access(access)
}

fn value<'a>(samples: &'a [edge_core::TelemetrySample], point_id: &str) -> &'a TelemetryValue {
    &samples
        .iter()
        .find(|sample| sample.telemetry_id == point_id)
        .unwrap_or_else(|| panic!("missing sample {point_id}"))
        .value
}

fn number(value: &TelemetryValue) -> f64 {
    match value {
        TelemetryValue::Integer(value) => *value as f64,
        TelemetryValue::Float(value) => *value,
        other => panic!("expected numeric telemetry, got {other:?}"),
    }
}

#[tokio::test]
#[ignore = "requires the Docker industrial protocol device lab"]
async fn production_s7_adapter_reads_dynamic_data_and_writes_command() {
    let endpoint = std::env::var("VELAEDGE_S7_SIM_ENDPOINT")
        .expect("VELAEDGE_S7_SIM_ENDPOINT must identify the container S7 device");
    let mappings = vec![
        mapping(
            "pressure",
            "s7-container",
            PointAddress::siemens_s7("DB1.REAL0"),
            TelemetryType::Float,
            PointAccess::ReadOnly,
        ),
        mapping(
            "running",
            "s7-container",
            PointAddress::siemens_s7("DB1.DBX4.0"),
            TelemetryType::Boolean,
            PointAccess::ReadOnly,
        ),
        mapping(
            "speed",
            "s7-container",
            PointAddress::siemens_s7("DB1.DINT6"),
            TelemetryType::Integer,
            PointAccess::ReadOnly,
        ),
        mapping(
            "start",
            "s7-container",
            PointAddress::siemens_s7("DB1.DBX10.0"),
            TelemetryType::Boolean,
            PointAccess::ReadWrite,
        ),
    ];
    let connection = ProtocolConnection::siemens_s7(
        "s7-container",
        endpoint,
        SiemensS7ConnectionSettings::default(),
    );
    let mut adapter = SiemensS7Adapter::new(connection, mappings.clone()).unwrap();

    let first = adapter.read_telemetry().await.unwrap();
    tokio::time::sleep(Duration::from_millis(600)).await;
    let second = adapter.read_telemetry().await.unwrap();
    assert_ne!(
        number(value(&first, "pressure")),
        number(value(&second, "pressure"))
    );
    assert_eq!(adapter.connection_generation(), 1);

    adapter
        .write_point(&mappings[3], TelemetryValue::Boolean(false))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(350)).await;
    let stopped = adapter.read_telemetry().await.unwrap();
    assert_eq!(value(&stopped, "start"), &TelemetryValue::Boolean(false));
    assert_eq!(value(&stopped, "running"), &TelemetryValue::Boolean(false));
    assert_eq!(value(&stopped, "speed"), &TelemetryValue::Integer(0));

    adapter
        .write_point(&mappings[3], TelemetryValue::Boolean(true))
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires the Docker industrial protocol device lab"]
async fn production_fins_adapter_reads_dynamic_data_and_writes_command() {
    let endpoint = std::env::var("VELAEDGE_FINS_SIM_ENDPOINT")
        .expect("VELAEDGE_FINS_SIM_ENDPOINT must identify the container FINS device");
    let mappings = vec![
        mapping(
            "counter",
            "fins-container",
            PointAddress::omron_fins("D100"),
            TelemetryType::Integer,
            PointAccess::ReadOnly,
        ),
        mapping(
            "temperature",
            "fins-container",
            PointAddress::omron_fins("D102"),
            TelemetryType::Float,
            PointAccess::ReadOnly,
        ),
        mapping(
            "running",
            "fins-container",
            PointAddress::omron_fins("CIO0.0"),
            TelemetryType::Boolean,
            PointAccess::ReadOnly,
        ),
        mapping(
            "start",
            "fins-container",
            PointAddress::omron_fins("CIO0.1"),
            TelemetryType::Boolean,
            PointAccess::ReadWrite,
        ),
    ];
    let connection = ProtocolConnection::omron_fins(
        "fins-container",
        format!("fins://{endpoint}"),
        OmronFinsConnectionSettings {
            transport: OmronFinsTransport::Tcp,
            source_node: 0,
            destination_node: 0,
            ..Default::default()
        },
    );
    let mut adapter = OmronFinsAdapter::new(connection, mappings.clone()).unwrap();

    let first = adapter.read_telemetry().await.unwrap();
    tokio::time::sleep(Duration::from_millis(600)).await;
    let second = adapter.read_telemetry().await.unwrap();
    assert!(number(value(&second, "counter")) > number(value(&first, "counter")));
    assert_ne!(
        number(value(&first, "temperature")),
        number(value(&second, "temperature"))
    );
    assert_eq!(adapter.connection_generation(), 1);

    adapter
        .write_point(&mappings[3], TelemetryValue::Boolean(false))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(350)).await;
    let stopped = adapter.read_telemetry().await.unwrap();
    assert_eq!(value(&stopped, "start"), &TelemetryValue::Boolean(false));
    assert_eq!(value(&stopped, "running"), &TelemetryValue::Boolean(false));

    adapter
        .write_point(&mappings[3], TelemetryValue::Boolean(true))
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires the Docker industrial protocol device lab"]
async fn production_iec104_adapter_reads_spontaneous_data_and_executes_all_controls() {
    let endpoint = std::env::var("VELAEDGE_IEC104_SIM_ENDPOINT")
        .expect("VELAEDGE_IEC104_SIM_ENDPOINT must identify the container IEC 104 device");
    let mappings = vec![
        mapping(
            "pressure",
            "iec104-container",
            PointAddress::iec104(1, 1001),
            TelemetryType::Float,
            PointAccess::ReadOnly,
        ),
        mapping(
            "running",
            "iec104-container",
            PointAddress::iec104(1, 1002),
            TelemetryType::Boolean,
            PointAccess::ReadOnly,
        ),
        mapping(
            "breaker_closed",
            "iec104-container",
            PointAddress::iec104(1, 1201),
            TelemetryType::Boolean,
            PointAccess::ReadWrite,
        )
        .with_iec104_options(
            Iec104PointOptions::new(Iec104ControlType::SingleCommand)
                .with_select_before_operate(true),
        ),
        mapping(
            "breaker_position",
            "iec104-container",
            PointAddress::iec104(1, 1202),
            TelemetryType::Integer,
            PointAccess::ReadWrite,
        )
        .with_iec104_options(Iec104PointOptions::new(Iec104ControlType::DoubleCommand)),
        mapping(
            "setpoint",
            "iec104-container",
            PointAddress::iec104(1, 1203),
            TelemetryType::Float,
            PointAccess::ReadWrite,
        )
        .with_iec104_options(Iec104PointOptions::new(Iec104ControlType::SetpointFloat)),
    ];
    let connection = ProtocolConnection::iec104("iec104-container", endpoint);
    let mut adapter = Iec104Adapter::new(connection, mappings.clone()).unwrap();

    let first = adapter.read_telemetry().await.unwrap();
    tokio::time::sleep(Duration::from_millis(750)).await;
    let second = adapter.read_telemetry().await.unwrap();
    assert_ne!(
        number(value(&first, "pressure")),
        number(value(&second, "pressure"))
    );
    assert_eq!(adapter.connection_generation(), 1);

    adapter
        .write_point(&mappings[2], TelemetryValue::Boolean(true))
        .await
        .unwrap();
    adapter
        .write_point(&mappings[3], TelemetryValue::Integer(2))
        .await
        .unwrap();
    adapter
        .write_point(&mappings[4], TelemetryValue::Float(18.75))
        .await
        .unwrap();

    let controlled = adapter.read_telemetry().await.unwrap();
    assert_eq!(
        value(&controlled, "breaker_closed"),
        &TelemetryValue::Boolean(true)
    );
    assert_eq!(
        value(&controlled, "breaker_position"),
        &TelemetryValue::Integer(2)
    );
    assert_eq!(
        value(&controlled, "setpoint"),
        &TelemetryValue::Float(18.75)
    );
    assert_eq!(adapter.connection_generation(), 1);
}

#[tokio::test]
#[ignore = "requires the Docker industrial protocol device lab"]
async fn production_bacnet_adapter_discovers_reads_cov_and_writes_property() {
    let endpoint = std::env::var("VELAEDGE_BACNET_SIM_ENDPOINT")
        .expect("VELAEDGE_BACNET_SIM_ENDPOINT must identify the container BACnet/IP device");
    let mappings = vec![
        mapping(
            "pressure",
            "bacnet-container",
            PointAddress::bacnet(42, "analog_input", 1, "present_value"),
            TelemetryType::Float,
            PointAccess::ReadOnly,
        ),
        mapping(
            "setpoint",
            "bacnet-container",
            PointAddress::bacnet(42, "analog_value", 7, "present_value"),
            TelemetryType::Float,
            PointAccess::ReadWrite,
        )
        .with_bacnet_options(BacnetPointOptions { write_priority: 8 }),
    ];
    let connection = ProtocolConnection::bacnet_ip(
        "bacnet-container",
        Some(endpoint),
        BacnetIpConnectionSettings {
            bind_address: "127.0.0.1".to_string(),
            apdu_timeout_ms: 1_500,
            apdu_retries: 1,
            cov: Some(BacnetCovSettings {
                lifetime_seconds: 300,
                confirmed_notifications: false,
                fallback_poll_interval_ms: 60_000,
            }),
            ..Default::default()
        },
    );
    let mut adapter = BacnetIpAdapter::new(connection, mappings.clone()).unwrap();

    let initial = adapter.read_telemetry().await.unwrap();
    assert_eq!(initial.len(), 2);
    let first_pressure = number(value(&initial, "pressure"));
    tokio::time::sleep(Duration::from_millis(1_100)).await;
    let changed = adapter.read_telemetry().await.unwrap();
    assert_ne!(number(value(&changed, "pressure")), first_pressure);
    assert_eq!(adapter.connection_generation(), 1);

    adapter
        .write_point(&mappings[1], TelemetryValue::Float(42.5))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(650)).await;
    let controlled = adapter.read_telemetry().await.unwrap();
    assert_eq!(value(&controlled, "setpoint"), &TelemetryValue::Float(42.5));
    assert_eq!(adapter.connection_generation(), 1);

    let metrics = adapter.cov_runtime_metrics();
    assert_eq!(metrics.active_subscriptions, 2);
    assert!(metrics.notifications_received >= 2);
    assert_eq!(metrics.subscription_failures, 0);
    assert_eq!(metrics.fallback_polls, 1);
}
