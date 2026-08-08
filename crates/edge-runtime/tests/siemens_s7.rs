use edge_core::{
    PointAccess, PointAddress, ProtocolConnection, SiemensS7ConnectionSettings,
    TelemetryPointMapping, TelemetryType, TelemetryValue,
};
use edge_runtime::{ProtocolAdapter, ProtocolCommandAdapter, SiemensS7Adapter};
use snap7_server::{area, DataStore, S7Server, ServerConfig};

fn mapping(
    point_id: &str,
    address: &str,
    value_type: TelemetryType,
    access: PointAccess,
) -> TelemetryPointMapping {
    TelemetryPointMapping::new(
        point_id,
        "plc-1",
        point_id,
        "s7-main",
        PointAddress::siemens_s7(address),
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

#[tokio::test]
async fn persistent_s7_session_reads_and_writes_real_tcp_server() {
    let store = DataStore::new();
    store.write_bytes(1, 0, &[0b1010_0100]);
    store.write_bytes(1, 4, &72.5_f32.to_be_bytes());
    store.write_bytes(1, 8, &(-42_i32).to_be_bytes());
    store.write_area(area::MARKERS, 0, 2, &1_234_u16.to_be_bytes());

    let server = S7Server::bind(ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        max_connections: 4,
    })
    .await
    .unwrap();
    let address = server.local_addr().unwrap();
    let server_store = store.clone();
    let server_task = tokio::spawn(async move { server.serve(server_store).await });

    let mappings = vec![
        mapping(
            "enabled",
            "DB1.DBX0.2",
            TelemetryType::Boolean,
            PointAccess::ReadWrite,
        ),
        mapping(
            "temperature",
            "DB1.REAL4",
            TelemetryType::Float,
            PointAccess::ReadOnly,
        ),
        mapping(
            "counter",
            "DB1.DINT8",
            TelemetryType::Integer,
            PointAccess::ReadWrite,
        ),
        mapping(
            "marker",
            "MW2",
            TelemetryType::Integer,
            PointAccess::ReadWrite,
        ),
    ];
    let connection = ProtocolConnection::siemens_s7(
        "s7-main",
        address.to_string(),
        SiemensS7ConnectionSettings::default(),
    );
    let mut adapter = SiemensS7Adapter::new(connection, mappings.clone()).unwrap();

    let samples = adapter.read_telemetry().await.unwrap();
    assert_eq!(value(&samples, "enabled"), &TelemetryValue::Boolean(true));
    assert_eq!(value(&samples, "temperature"), &TelemetryValue::Float(72.5));
    assert_eq!(value(&samples, "counter"), &TelemetryValue::Integer(-42));
    assert_eq!(value(&samples, "marker"), &TelemetryValue::Integer(1_234));
    assert_eq!(adapter.connection_generation(), 1);

    store.write_bytes(1, 4, &81.25_f32.to_be_bytes());
    let samples = adapter.read_telemetry().await.unwrap();
    assert_eq!(
        value(&samples, "temperature"),
        &TelemetryValue::Float(81.25)
    );
    assert_eq!(adapter.connection_generation(), 1);

    adapter
        .write_point(&mappings[2], TelemetryValue::Integer(7_654))
        .await
        .unwrap();
    assert_eq!(
        store.read_bytes(1, 8, 4),
        7_654_i32.to_be_bytes().as_slice()
    );

    adapter
        .write_point(&mappings[0], TelemetryValue::Boolean(false))
        .await
        .unwrap();
    assert_eq!(store.read_bytes(1, 0, 1), [0b1010_0000]);
    assert_eq!(adapter.connection_generation(), 1);

    let error = adapter
        .write_point(&mappings[1], TelemetryValue::Float(10.0))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not writable"));

    server_task.abort();
}

#[test]
fn s7_adapter_rejects_mappings_from_another_connection() {
    let mut foreign = mapping(
        "counter",
        "DB1.DINT8",
        TelemetryType::Integer,
        PointAccess::ReadOnly,
    );
    foreign.protocol_connection_id = "other".to_string();
    let connection = ProtocolConnection::siemens_s7(
        "s7-main",
        "127.0.0.1:102",
        SiemensS7ConnectionSettings::default(),
    );
    assert!(SiemensS7Adapter::new(connection, vec![foreign]).is_err());
}
