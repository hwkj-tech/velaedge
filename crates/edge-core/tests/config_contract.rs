use edge_core::{
    bacnet_object_templates, bacnet_property_templates, dlt645_data_identifier_templates,
    dlt645_template_by_identifier, parse_bacnet_point_address, parse_dlt645_point_address,
    AlgorithmDsl, AlgorithmInputBinding, AlgorithmKind, AlgorithmOutput, AlgorithmReportMode,
    AlgorithmReportPolicy, AlgorithmRuntime, AlgorithmSpec, AlgorithmStep, AlgorithmTrigger,
    BacnetCovSettings, BacnetForeignDeviceSettings, BacnetIpConnectionSettings, CollectionTask,
    CompareOperator, CustomSerialChecksum, CustomSerialFrameEncoding, CustomSerialPointSpec,
    CustomSerialValueEncoding, DataConfig, DataConfigCollection, DataConfigPayload,
    DataConfigPoint, DataConfigPublish, DeviceInstance, DiscoveryAddressKind, DiscoveryRequest,
    EdgeConfigPackage, Iec101ControlType, Iec101PointOptions, Iec104ConnectionSettings,
    Iec104ControlType, Iec104PointOptions, ModbusByteOrder, ModbusPointOptions,
    ModbusRegisterEncoding, ModbusWordOrder, MqttLastWillConfig, MqttProtocolVersion,
    MqttUplinkConfig, MqttUserProperty, NumberRange, OmronFinsArea, OmronFinsConnectionSettings,
    OmronFinsTransport, OmronFinsWordOrder, OpcUaAuthMode, OpcUaConnectionSettings,
    OpcUaMessageSecurityMode, OpcUaPointOptions, OpcUaSecurityPolicy, OpcUaWriteDataType,
    PointAccess, PointAddress, ProtocolCircuitBreakerConfig, ProtocolConnection, ProtocolType,
    SerialConnectionSettings, SiemensS7Area, SiemensS7ConnectionSettings, SiemensS7DataType,
    TelemetryPointMapping, TelemetryType, WindowAggregateFunction,
};

#[test]
fn omron_fins_connection_and_point_contracts_are_strict() {
    let settings = OmronFinsConnectionSettings {
        source_node: 11,
        destination_node: 25,
        timeout_ms: 3_500,
        word_order: OmronFinsWordOrder::HighWordFirst,
        ..Default::default()
    };
    let connection =
        ProtocolConnection::omron_fins("fins-line-1", "fins://127.0.0.1:9600", settings.clone());
    connection.validate().expect("FINS connection is valid");
    assert_eq!(connection.protocol, ProtocolType::OmronFins);

    let json = serde_json::to_value(&connection).expect("connection serializes");
    assert_eq!(json["omron_fins"]["transport"], "udp");
    assert_eq!(json["omron_fins"]["sourceNode"], 11);
    assert_eq!(json["omron_fins"]["destinationNode"], 25);
    assert_eq!(json["omron_fins"]["wordOrder"], "high_word_first");
    assert_eq!(
        serde_json::from_value::<ProtocolConnection>(json)
            .expect("connection deserializes")
            .omron_fins,
        Some(settings)
    );

    let legacy_settings: OmronFinsConnectionSettings = serde_json::from_value(serde_json::json!({
        "sourceNode": 1,
        "destinationNode": 10,
        "timeoutMs": 2000,
        "wordOrder": "low_word_first"
    }))
    .expect("legacy FINS settings without transport remain readable");
    assert_eq!(legacy_settings.transport, OmronFinsTransport::Udp);

    let work = edge_core::parse_omron_fins_point_address("WR10.3").unwrap();
    assert_eq!(work.area, OmronFinsArea::Work);
    assert_eq!(work.canonical(), "W10.3");
    let data = edge_core::validate_omron_fins_point(
        &PointAddress::omron_fins("DM100"),
        TelemetryType::Integer,
        PointAccess::ReadWrite,
    )
    .unwrap();
    assert_eq!(data.canonical(), "D100");

    assert!(edge_core::validate_omron_fins_point(
        &PointAddress::omron_fins("CIO0"),
        TelemetryType::Boolean,
        PointAccess::ReadOnly,
    )
    .is_err());
    assert!(edge_core::validate_omron_fins_point(
        &PointAddress::omron_fins("D100.1"),
        TelemetryType::Boolean,
        PointAccess::ReadOnly,
    )
    .is_err());
    assert!(ProtocolConnection::omron_fins(
        "bad",
        "fins://127.0.0.1",
        OmronFinsConnectionSettings::default(),
    )
    .validate()
    .is_err());

    let tcp = ProtocolConnection::omron_fins(
        "fins-tcp",
        "fins://127.0.0.1:9600",
        OmronFinsConnectionSettings {
            transport: OmronFinsTransport::Tcp,
            source_node: 0,
            destination_node: 0,
            ..Default::default()
        },
    );
    tcp.validate()
        .expect("FINS/TCP permits handshake-assigned source and destination nodes");

    let invalid_udp = ProtocolConnection::omron_fins(
        "fins-udp",
        "fins://127.0.0.1:9600",
        OmronFinsConnectionSettings {
            source_node: 0,
            ..Default::default()
        },
    );
    assert!(invalid_udp.validate().is_err());
}

#[test]
fn siemens_s7_connection_and_point_contracts_are_strict() {
    let connection = ProtocolConnection::siemens_s7(
        "plc-line-1",
        "s7://127.0.0.1:102",
        SiemensS7ConnectionSettings::default(),
    );
    connection
        .validate()
        .expect("Siemens S7 connection is valid");
    assert_eq!(connection.protocol, ProtocolType::SiemensS7);

    let db_real = edge_core::validate_siemens_s7_point(
        &PointAddress::siemens_s7("db1.real4"),
        TelemetryType::Float,
        PointAccess::ReadOnly,
    )
    .unwrap();
    assert_eq!(db_real.canonical(), "DB1.REAL4");
    assert_eq!(db_real.area, SiemensS7Area::DataBlock);
    assert_eq!(db_real.data_type, SiemensS7DataType::Real);
    assert_eq!(db_real.byte_offset, 4);

    let marker_bit = edge_core::parse_siemens_s7_point_address("M10.3").unwrap();
    assert_eq!(marker_bit.data_type, SiemensS7DataType::Bit);
    assert_eq!(marker_bit.bit_offset, Some(3));
    assert_eq!(marker_bit.canonical(), "M10.3");

    let input = PointAddress::siemens_s7("IW2");
    assert!(edge_core::validate_siemens_s7_point(
        &input,
        TelemetryType::Integer,
        PointAccess::ReadWrite
    )
    .is_err());
    assert!(edge_core::validate_siemens_s7_point(
        &PointAddress::siemens_s7("DB1.DBX0.8"),
        TelemetryType::Boolean,
        PointAccess::ReadOnly
    )
    .is_err());
    assert!(edge_core::validate_siemens_s7_point(
        &PointAddress::siemens_s7("DB1.DBW2"),
        TelemetryType::Float,
        PointAccess::ReadOnly
    )
    .is_err());
    assert!(ProtocolConnection::siemens_s7(
        "bad",
        "s7://127.0.0.1",
        SiemensS7ConnectionSettings::default()
    )
    .validate()
    .is_err());
}

#[test]
fn bacnet_ip_connection_and_point_contracts_are_structured() {
    let connection = ProtocolConnection::bacnet_ip(
        "building-a",
        Some("bacnet://127.0.0.1:47808"),
        BacnetIpConnectionSettings::default(),
    );
    connection
        .validate()
        .expect("BACnet/IP connection is valid");
    assert_eq!(connection.protocol, ProtocolType::BacnetIp);

    let address = PointAddress::bacnet(1234, "analog_input", 1, "present_value");
    assert_eq!(address.kind, "bacnet_object_property");
    let parsed = parse_bacnet_point_address(&address.value).unwrap();
    assert_eq!(parsed.device_instance, 1234);
    assert_eq!(parsed.object_type, 0);
    assert_eq!(parsed.object_instance, 1);
    assert_eq!(parsed.property_identifier, 85);
    assert_eq!(parsed.array_index, None);
    assert!(bacnet_object_templates().len() >= 10);
    assert!(bacnet_property_templates().len() >= 7);
    assert!(parse_bacnet_point_address("1234:unknown:1:present_value").is_err());
    assert!(parse_bacnet_point_address("4194303:analog_input:1:85").is_err());

    let foreign_device = BacnetIpConnectionSettings {
        foreign_device: Some(BacnetForeignDeviceSettings {
            bbmd_address: "10.12.0.10:47808".to_string(),
            ttl_seconds: 120,
        }),
        ..BacnetIpConnectionSettings::default()
    };
    foreign_device
        .validate()
        .expect("valid BBMD foreign device settings");

    let cov = BacnetIpConnectionSettings {
        cov: Some(BacnetCovSettings {
            lifetime_seconds: 300,
            confirmed_notifications: true,
            fallback_poll_interval_ms: 60_000,
        }),
        ..BacnetIpConnectionSettings::default()
    };
    cov.validate().expect("valid BACnet COV settings");

    let invalid_ttl = BacnetIpConnectionSettings {
        foreign_device: Some(BacnetForeignDeviceSettings {
            bbmd_address: "10.12.0.10:47808".to_string(),
            ttl_seconds: 10,
        }),
        ..BacnetIpConnectionSettings::default()
    };
    assert!(invalid_ttl.validate().is_err());

    let invalid_cov = BacnetIpConnectionSettings {
        cov: Some(BacnetCovSettings {
            lifetime_seconds: 30,
            confirmed_notifications: false,
            fallback_poll_interval_ms: 500,
        }),
        ..BacnetIpConnectionSettings::default()
    };
    assert!(invalid_cov.validate().is_err());
}

#[test]
fn dlt645_catalog_and_address_contract_cover_common_meter_data() {
    let templates = dlt645_data_identifier_templates();
    assert!(templates.len() >= 16);
    assert_eq!(
        templates
            .iter()
            .map(|template| template.template_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        templates.len()
    );

    let voltage = dlt645_template_by_identifier(0x0201_0100).unwrap();
    assert_eq!(voltage.name, "A 相电压");
    assert_eq!(voltage.decimal_places, 1);
    assert_eq!(voltage.value_bytes, 2);
    assert_eq!(voltage.unit, Some("V"));

    let address = parse_dlt645_point_address("123456789012:02010100:1").unwrap();
    assert_eq!(address.meter_address, "123456789012");
    assert_eq!(address.data_identifier, 0x0201_0100);
    assert_eq!(address.decimal_places, 1);
    assert_eq!(address.value_bytes, None);

    let vendor = parse_dlt645_point_address("123456789012:F0010203:2:4").unwrap();
    assert_eq!(vendor.data_identifier, 0xF001_0203);
    assert_eq!(vendor.decimal_places, 2);
    assert_eq!(vendor.value_bytes, Some(4));
    assert_eq!(
        PointAddress::dlt645_vendor("123456789012", "F0010203", 2, 4).value,
        "123456789012:F0010203:2:4"
    );
    assert!(parse_dlt645_point_address("123:02010100:1").is_err());
    assert!(parse_dlt645_point_address("123456789012:0201010Z:1").is_err());
    assert!(parse_dlt645_point_address("123456789012:F0010203:2:0").is_err());
    assert!(parse_dlt645_point_address("123456789012:F0010203:2:252").is_err());
    assert!(parse_dlt645_point_address("123456789012:F0010203:2:four").is_err());
    assert!(parse_dlt645_point_address("123456789012:F0010203:2:4:extra").is_err());
}

#[test]
fn iec104_connection_and_point_address_contracts_are_strict() {
    let settings = Iec104ConnectionSettings::default().with_cp56_timezone_offset_minutes(480);
    let connection = ProtocolConnection::iec104("substation-a", "tcp://127.0.0.1:2404")
        .with_iec104_settings(settings);
    connection.validate().expect("IEC 104 connection is valid");
    assert_eq!(connection.protocol, ProtocolType::Iec104);
    assert_eq!(connection.iec104, Some(settings));

    let json = serde_json::to_value(&connection).expect("IEC 104 connection serializes");
    assert_eq!(json["iec104"]["cp56TimeZoneOffsetMinutes"], 480);

    let address = PointAddress::iec104(2, 1001);
    assert_eq!(address.kind, "iec104_ioa");
    assert_eq!(address.value, "2:1001");
    assert_eq!(
        edge_core::parse_iec104_point_address(&address.value),
        Ok((2, 1001))
    );

    assert!(ProtocolConnection::iec104("bad", "http://127.0.0.1:2404")
        .validate()
        .is_err());
    assert!(ProtocolConnection::iec104("bad-timezone", "127.0.0.1:2404")
        .with_iec104_settings(
            Iec104ConnectionSettings::default().with_cp56_timezone_offset_minutes(841),
        )
        .validate()
        .is_err());
    assert!(ProtocolConnection::modbus_tcp("modbus", "127.0.0.1:502")
        .with_iec104_settings(settings)
        .validate()
        .is_err());
    assert!(edge_core::parse_iec104_point_address("0:1001").is_err());
    assert!(edge_core::parse_iec104_point_address("2:16777216").is_err());
}

#[test]
fn writable_iec104_points_require_an_exact_control_type() {
    let address = PointAddress::iec104(2, 1001);
    assert!(edge_core::validate_iec104_point(
        &address,
        TelemetryType::Boolean,
        PointAccess::ReadWrite,
        None,
    )
    .unwrap_err()
    .contains("controlType"));
    assert!(edge_core::validate_iec104_point(
        &address,
        TelemetryType::Boolean,
        PointAccess::ReadWrite,
        Some(Iec104PointOptions::new(Iec104ControlType::SingleCommand)),
    )
    .is_ok());
    assert!(edge_core::validate_iec104_point(
        &address,
        TelemetryType::Float,
        PointAccess::ReadWrite,
        Some(Iec104PointOptions::new(Iec104ControlType::SingleCommand)),
    )
    .unwrap_err()
    .contains("incompatible"));
    assert!(edge_core::validate_iec104_point(
        &address,
        TelemetryType::Text,
        PointAccess::ReadOnly,
        None,
    )
    .is_ok());

    let mapping = TelemetryPointMapping::new(
        "breaker_control",
        "substation-1",
        "breaker.control",
        "iec104-main",
        address,
        TelemetryType::Boolean,
    )
    .with_access(PointAccess::ReadWrite)
    .with_iec104_options(
        Iec104PointOptions::new(Iec104ControlType::SingleCommand).with_select_before_operate(true),
    );
    let json = serde_json::to_value(mapping).expect("IEC 104 mapping serializes");
    assert_eq!(json["iec104"]["controlType"], "C_SC_NA_1");
    assert_eq!(json["iec104"]["selectBeforeOperate"], true);
}

#[test]
fn writable_iec101_points_require_station_address_and_exact_control_type() {
    let address = PointAddress::iec101(1, 2, 1001);
    assert_eq!(
        edge_core::parse_iec101_point_address(&address.value),
        Ok((1, 2, 1001))
    );
    assert!(edge_core::parse_iec101_point_address("255:2:1001").is_err());
    assert!(edge_core::parse_iec101_point_address("1:0:1001").is_err());
    assert!(edge_core::parse_iec101_point_address("1:2:16777216").is_err());

    assert!(edge_core::validate_iec101_point(
        &address,
        TelemetryType::Boolean,
        PointAccess::ReadWrite,
        None,
    )
    .unwrap_err()
    .contains("controlType"));
    assert!(edge_core::validate_iec101_point(
        &address,
        TelemetryType::Boolean,
        PointAccess::ReadWrite,
        Some(Iec101PointOptions::new(Iec101ControlType::SingleCommand)),
    )
    .is_ok());
    assert!(edge_core::validate_iec101_point(
        &address,
        TelemetryType::Float,
        PointAccess::ReadWrite,
        Some(Iec101PointOptions::new(Iec101ControlType::DoubleCommand)),
    )
    .unwrap_err()
    .contains("incompatible"));

    let mapping = TelemetryPointMapping::new(
        "breaker_control",
        "substation-1",
        "breaker.control",
        "iec101-main",
        address,
        TelemetryType::Boolean,
    )
    .with_access(PointAccess::ReadWrite)
    .with_iec101_options(
        Iec101PointOptions::new(Iec101ControlType::SingleCommand).with_select_before_operate(true),
    );
    let json = serde_json::to_value(mapping).expect("IEC 101 mapping serializes");
    assert_eq!(json["iec101"]["controlType"], "C_SC_NA_1");
    assert_eq!(json["iec101"]["selectBeforeOperate"], true);
}

#[test]
fn config_package_contains_edge_targets_and_point_mappings() {
    let package = EdgeConfigPackage::new("edge-dev", "2026.06.26-001")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_point_mapping(
            TelemetryPointMapping::new(
                "pressure",
                "pump-1",
                "pressure",
                "sim-main",
                PointAddress::simulated("pressure"),
                TelemetryType::Float,
            )
            .with_unit("MPa")
            .with_range(NumberRange::new(0.0, 20.0))
            .with_interval_ms(1000),
        )
        .with_collection_task(CollectionTask::interval(
            "pump-main-collection",
            "pump-1",
            vec!["pressure".to_string()],
            1000,
        ));

    assert_eq!(package.edge_id, "edge-dev");
    assert_eq!(package.version, "2026.06.26-001");
    assert_eq!(package.point_mappings[0].point_id, "pressure");
    assert_eq!(
        package.protocol_connections[0].protocol,
        ProtocolType::Simulated
    );
    assert_eq!(package.collection_tasks[0].point_ids, vec!["pressure"]);
}

#[test]
fn modbus_point_address_preserves_register_metadata() {
    let address = PointAddress::modbus_holding_register(40001);

    assert_eq!(address.kind, "holding_register");
    assert_eq!(address.value, "40001");
}

#[test]
fn opc_ua_connection_round_trips_security_and_authentication_settings() {
    let settings = OpcUaConnectionSettings {
        security_policy: OpcUaSecurityPolicy::Basic256Sha256,
        message_security_mode: OpcUaMessageSecurityMode::SignAndEncrypt,
        auth_mode: OpcUaAuthMode::Username,
        username: Some("operator".to_string()),
        password_env: Some("VELAEDGE_OPCUA_PASSWORD".to_string()),
        trust_server_certs: true,
        verify_server_certs: false,
        request_timeout_ms: 8_000,
        ..Default::default()
    };
    let connection =
        ProtocolConnection::opc_ua("line-opcua", "opc.tcp://plc.local:4840", settings.clone());

    connection.validate().expect("OPC UA connection is valid");
    let json = serde_json::to_value(&connection).expect("connection serializes");
    assert_eq!(json["opc_ua"]["securityPolicy"], "basic256_sha256");
    assert_eq!(json["opc_ua"]["authMode"], "username");
    assert_eq!(json["opc_ua"]["requestTimeoutMs"], 8_000);
    assert_eq!(
        serde_json::from_value::<ProtocolConnection>(json)
            .expect("connection deserializes")
            .opc_ua,
        Some(settings)
    );
}

#[test]
fn opc_ua_connection_rejects_incompatible_security_and_authentication() {
    let insecure_signed = OpcUaConnectionSettings {
        message_security_mode: OpcUaMessageSecurityMode::Sign,
        ..Default::default()
    };
    assert!(
        ProtocolConnection::opc_ua("line-opcua", "opc.tcp://localhost:4840", insecure_signed,)
            .validate()
            .unwrap_err()
            .contains("None security policy")
    );

    let incomplete_username = OpcUaConnectionSettings {
        auth_mode: OpcUaAuthMode::Username,
        username: Some("operator".to_string()),
        ..Default::default()
    };
    assert!(ProtocolConnection::opc_ua(
        "line-opcua",
        "opc.tcp://localhost:4840",
        incomplete_username,
    )
    .validate()
    .unwrap_err()
    .contains("password environment variable"));
}

#[test]
fn opc_ua_node_id_contract_accepts_standard_identifier_forms() {
    for node_id in [
        "i=2258",
        "ns=2;s=Machine/Speed",
        "ns=3;g=abc",
        "ns=4;b=AQID",
    ] {
        edge_core::validate_opc_ua_node_id(node_id).expect("NodeId is valid");
        assert_eq!(PointAddress::opc_ua_node_id(node_id).kind, "node_id");
    }
    for node_id in ["", "ns=x;s=Speed", "ns=2;x=Speed", "ns=2;i=abc"] {
        assert!(edge_core::validate_opc_ua_node_id(node_id).is_err());
    }
}

#[test]
fn opc_ua_browse_path_contract_round_trips_qualified_names() {
    let path = edge_core::OpcUaBrowsePathAddress::new(
        "i=85",
        vec![
            edge_core::OpcUaBrowsePathElement::new(0, "Server"),
            edge_core::OpcUaBrowsePathElement::new(0, "ServiceLevel"),
        ],
    );
    let address = PointAddress::opc_ua_browse_path(&path).expect("BrowsePath serializes");

    assert_eq!(address.kind, "browse_path");
    assert_eq!(
        edge_core::parse_opc_ua_browse_path(&address.value).expect("BrowsePath parses"),
        path
    );
    assert!(
        edge_core::parse_opc_ua_browse_path(r#"{"startingNode":"i=85","elements":[]}"#).is_err()
    );
}

#[test]
fn modbus_point_options_round_trip_with_backward_compatible_defaults() {
    let address =
        PointAddress::modbus_holding_register(40001).with_modbus_options(ModbusPointOptions {
            encoding: Some(ModbusRegisterEncoding::F32),
            byte_order: ModbusByteOrder::LittleEndian,
            word_order: ModbusWordOrder::LowWordFirst,
            scale: 0.1,
            offset: -5.0,
            bit_index: None,
        });
    let json = serde_json::to_value(&address).unwrap();
    assert_eq!(json["modbus"]["encoding"], "f32");
    assert_eq!(json["modbus"]["byteOrder"], "little_endian");
    assert_eq!(json["modbus"]["wordOrder"], "low_word_first");
    assert_eq!(
        serde_json::from_value::<PointAddress>(json).unwrap(),
        address
    );

    let legacy: PointAddress = serde_json::from_value(serde_json::json!({
        "kind": "holding_register",
        "value": "40001"
    }))
    .unwrap();
    assert_eq!(legacy.modbus, None);
}

#[test]
fn modbus_point_options_reject_unsafe_or_incompatible_combinations() {
    let bit_field =
        PointAddress::modbus_holding_register(40001).with_modbus_options(ModbusPointOptions {
            bit_index: Some(16),
            ..Default::default()
        });
    assert!(edge_core::validate_modbus_point_options(
        &bit_field,
        TelemetryType::Boolean,
        PointAccess::ReadOnly,
    )
    .unwrap_err()
    .contains("between 0 and 15"));

    let writable_bit =
        PointAddress::modbus_holding_register(40001).with_modbus_options(ModbusPointOptions {
            bit_index: Some(3),
            ..Default::default()
        });
    assert!(edge_core::validate_modbus_point_options(
        &writable_bit,
        TelemetryType::Boolean,
        PointAccess::ReadWrite,
    )
    .unwrap_err()
    .contains("atomic mask-write"));
}

#[test]
fn point_access_defaults_to_read_only_and_round_trips_explicit_write_permission() {
    let legacy: TelemetryPointMapping = serde_json::from_value(serde_json::json!({
        "point_id": "pressure",
        "device_id": "pump-1",
        "semantic_id": "pressure",
        "protocol_connection_id": "modbus-main",
        "address": {
            "kind": "holding_register",
            "value": "40001"
        },
        "value_type": "Float",
        "unit": "MPa",
        "range": null,
        "interval_ms": 1000
    }))
    .unwrap();

    assert_eq!(legacy.access, PointAccess::ReadOnly);
    assert!(!legacy.access.is_writable());

    let writable = legacy.with_access(PointAccess::ReadWrite);
    let json = serde_json::to_value(&writable).unwrap();
    assert_eq!(json["access"], "read_write");

    let decoded: TelemetryPointMapping = serde_json::from_value(json).unwrap();
    assert_eq!(decoded.access, PointAccess::ReadWrite);
    assert!(decoded.access.is_readable());
    assert!(decoded.access.is_writable());
}

#[test]
fn modbus_rtu_connection_preserves_serial_settings() {
    let serial = SerialConnectionSettings::new("/dev/ttyUSB0", 9600)
        .with_data_bits(8)
        .with_stop_bits(1)
        .with_parity("none");
    let connection = ProtocolConnection::modbus_rtu_serial("meter-rs485-bus-1", serial.clone());

    assert_eq!(connection.protocol, ProtocolType::ModbusRtu);
    assert_eq!(connection.endpoint.as_deref(), Some("/dev/ttyUSB0"));
    assert_eq!(connection.serial.as_ref(), Some(&serial));
}

#[test]
fn modbus_tcp_connection_preserves_network_endpoint() {
    let connection = ProtocolConnection::modbus_tcp("plc-main", "tcp://127.0.0.1:1502");

    assert_eq!(connection.protocol, ProtocolType::ModbusTcp);
    assert_eq!(connection.endpoint.as_deref(), Some("tcp://127.0.0.1:1502"));
    assert!(connection.serial.is_none());
}

#[test]
fn protocol_circuit_breaker_round_trips_and_legacy_connections_use_safe_defaults() {
    let connection = ProtocolConnection::modbus_tcp("plc-main", "127.0.0.1:502")
        .with_circuit_breaker(ProtocolCircuitBreakerConfig {
            enabled: true,
            failure_threshold: 3,
            open_duration_ms: 15_000,
            half_open_success_threshold: 2,
        });
    let json = serde_json::to_value(&connection).unwrap();
    assert_eq!(json["circuit_breaker"]["failureThreshold"], 3);
    assert_eq!(json["circuit_breaker"]["openDurationMs"], 15_000);
    assert_eq!(
        serde_json::from_value::<ProtocolConnection>(json).unwrap(),
        connection
    );

    let legacy: ProtocolConnection = serde_json::from_value(serde_json::json!({
        "connection_id": "legacy-modbus",
        "protocol": "ModbusTcp",
        "endpoint": "127.0.0.1:502"
    }))
    .unwrap();
    assert_eq!(
        legacy.circuit_breaker,
        ProtocolCircuitBreakerConfig::default()
    );
}

#[test]
fn iec101_connection_and_point_preserve_serial_and_address_metadata() {
    let serial = SerialConnectionSettings::new("/dev/ttyUSB1", 9600)
        .with_data_bits(8)
        .with_stop_bits(1)
        .with_parity("even");
    let connection = ProtocolConnection::iec101_serial("substation-iec101", serial.clone())
        .with_iec101_settings(
            edge_core::Iec101ConnectionSettings::default().with_cp56_timezone_offset_minutes(480),
        );
    let address = PointAddress::iec101(1, 2, 1001);

    assert_eq!(connection.protocol, ProtocolType::Iec101);
    assert_eq!(connection.serial.as_ref(), Some(&serial));
    assert_eq!(
        connection
            .iec101
            .expect("IEC 101 settings")
            .cp56_timezone_offset_minutes,
        480
    );
    connection.validate().expect("IEC 101 connection is valid");
    assert_eq!(
        serde_json::to_value(&connection).unwrap()["iec101"]["cp56TimeZoneOffsetMinutes"],
        480
    );
    assert_eq!(address.kind, "iec101_ioa");
    assert_eq!(address.value, "1:2:1001");
}

#[test]
fn iec101_connection_rejects_invalid_or_cross_protocol_timezone_settings() {
    let invalid = ProtocolConnection::iec101_serial(
        "substation-iec101",
        SerialConnectionSettings::new("/dev/ttyUSB1", 9600),
    )
    .with_iec101_settings(
        edge_core::Iec101ConnectionSettings::default().with_cp56_timezone_offset_minutes(841),
    );
    assert!(invalid
        .validate()
        .unwrap_err()
        .contains("between -840 and 840"));

    let mut modbus = ProtocolConnection::modbus_tcp("modbus-main", "127.0.0.1:502");
    modbus.iec101 = Some(edge_core::Iec101ConnectionSettings::default());
    assert!(modbus
        .validate()
        .unwrap_err()
        .contains("only valid for IEC 101"));
}

#[test]
fn custom_serial_point_address_is_a_validated_structured_contract() {
    let mut spec = CustomSerialPointSpec::new("01 03 00 10", 3, CustomSerialValueEncoding::U16Be);
    spec.request_checksum = CustomSerialChecksum::ModbusCrc16;
    spec.response_checksum = CustomSerialChecksum::Sum8;
    spec.response_prefix_hex = Some("01:03".to_string());
    spec.scale = 0.1;

    edge_core::validate_custom_serial_point_spec(&spec).expect("spec is valid");
    let address = PointAddress::custom_serial(&spec).expect("address serializes");
    let decoded: CustomSerialPointSpec = serde_json::from_str(&address.value).unwrap();

    assert_eq!(address.kind, "custom_serial_frame");
    assert_eq!(decoded, spec);
    assert_eq!(decoded.value_width().unwrap(), 2);
}

#[test]
fn custom_serial_contract_rejects_unbounded_or_malformed_frames() {
    let mut malformed = CustomSerialPointSpec::new("0A1", 0, CustomSerialValueEncoding::U16Be);
    assert!(edge_core::validate_custom_serial_point_spec(&malformed)
        .unwrap_err()
        .contains("complete byte pairs"));

    malformed.request_hex = "01".to_string();
    malformed.value_offset = 4095;
    assert!(edge_core::validate_custom_serial_point_spec(&malformed)
        .unwrap_err()
        .contains("4096-byte response limit"));

    malformed.value_offset = 0;
    malformed.schema_version = 3;
    assert!(edge_core::validate_custom_serial_point_spec(&malformed)
        .unwrap_err()
        .contains("supported versions are 1 and 2"));

    malformed.schema_version = 1;
    malformed.frame_encoding = CustomSerialFrameEncoding::Slip;
    assert!(edge_core::validate_custom_serial_point_spec(&malformed)
        .unwrap_err()
        .contains("schemaVersion 1 only supports raw"));

    malformed.schema_version = 2;
    edge_core::validate_custom_serial_point_spec(&malformed)
        .expect("schema version 2 supports framed transports");
}

#[test]
fn legacy_custom_serial_json_defaults_to_v1_raw_frames_and_rejects_unknown_fields() {
    let legacy = serde_json::json!({
        "requestHex": "01",
        "valueOffset": 0,
        "valueLength": 1,
        "valueEncoding": "u8"
    });
    let spec: CustomSerialPointSpec = serde_json::from_value(legacy).unwrap();
    assert_eq!(spec.schema_version, 1);
    assert_eq!(spec.frame_encoding, CustomSerialFrameEncoding::Raw);

    let error = serde_json::from_value::<CustomSerialPointSpec>(serde_json::json!({
        "schemaVersion": 2,
        "requestHex": "01",
        "valueOffset": 0,
        "valueLength": 1,
        "valueEncoding": "u8",
        "vendorMagic": true
    }))
    .unwrap_err();
    assert!(error.to_string().contains("unknown field"));
}

#[test]
fn mqtt_is_modeled_as_northbound_uplink_not_device_protocol() {
    let protocol_json =
        serde_json::to_string(&ProtocolType::ModbusRtu).expect("protocol serializes");

    assert_ne!(protocol_json, "\"Mqtt\"");

    let uplink = MqttUplinkConfig::velamq(
        "velamq-main",
        "mqtts://velamq.local:8883",
        "edge-dev-runtime-dev",
    )
    .with_topic_template("edge/{edge_id}/device/{device_id}/telemetry")
    .with_qos(1);

    assert_eq!(uplink.sink_id, "velamq-main");
    assert_eq!(uplink.broker, "mqtts://velamq.local:8883");
    assert_eq!(uplink.qos, 1);
    assert_eq!(uplink.username, None);
    assert_eq!(uplink.password_env, None);
    assert_eq!(uplink.tls_ca_path, None);
    assert_eq!(
        uplink.topic_template,
        "edge/{edge_id}/device/{device_id}/telemetry"
    );
}

#[test]
fn mqtt_security_uses_secret_references_and_keeps_legacy_json_compatible() {
    let secured = MqttUplinkConfig::velamq(
        "velamq-main",
        "mqtts://velamq.local:8883",
        "edge-dev-runtime-dev",
    )
    .with_credentials_env("edge-device", "EDGEOPS_MQTT_PASSWORD")
    .with_tls_ca_path("/etc/edgeops/velamq-ca.pem");
    let json = serde_json::to_value(&secured).unwrap();

    assert_eq!(json["username"], "edge-device");
    assert_eq!(json["password_env"], "EDGEOPS_MQTT_PASSWORD");
    assert!(json.get("password").is_none());
    assert_eq!(json["tls_ca_path"], "/etc/edgeops/velamq-ca.pem");

    let legacy: MqttUplinkConfig = serde_json::from_value(serde_json::json!({
        "sink_id": "legacy",
        "broker": "mqtt://127.0.0.1:1883",
        "client_id": "legacy-client",
        "topic_template": "edge/{edge_id}/telemetry",
        "qos": 1,
        "batch_size": 100,
        "flush_interval_ms": 1000
    }))
    .unwrap();
    assert_eq!(legacy.username, None);
    assert_eq!(legacy.password_env, None);
    assert_eq!(legacy.tls_ca_path, None);
    assert_eq!(legacy.protocol_version, MqttProtocolVersion::V3_1_1);
    assert_eq!(legacy.keep_alive_seconds, 60);
    assert!(legacy.clean_session);
}

#[test]
fn mqtt_5_session_settings_are_part_of_the_runtime_contract() {
    let mut uplink =
        MqttUplinkConfig::velamq("velamq-main", "mqtt://127.0.0.1:1883", "edge-runtime")
            .with_protocol_version(MqttProtocolVersion::V5_0);
    uplink.clean_start = false;
    uplink.session_expiry_interval_seconds = 3600;

    let json = serde_json::to_value(&uplink).unwrap();
    assert_eq!(json["protocol_version"], "5.0");
    assert_eq!(json["clean_start"], false);
    assert_eq!(json["session_expiry_interval_seconds"], 3600);

    let decoded: MqttUplinkConfig = serde_json::from_value(json).unwrap();
    assert_eq!(decoded.protocol_version, MqttProtocolVersion::V5_0);
    assert!(!decoded.clean_start);
    assert_eq!(decoded.session_expiry_interval_seconds, 3600);
}

#[test]
fn config_package_contains_data_configs_for_grouped_mqtt_publishing() {
    let package = EdgeConfigPackage::new("edge-dev", "v1")
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtts://velamq.local:8883",
            "edge-dev-runtime",
        ))
        .with_data_config(
            DataConfig::new(
                "pump_status",
                "泵运行状态上报",
                "pump-1",
                "modbus-line-a",
                DataConfigCollection::new(1000),
                DataConfigPublish::new(
                    "velamq-main",
                    "factory/{site}/pump/{device_id}/status",
                    DataConfigPayload::object(),
                ),
            )
            .with_point(DataConfigPoint::new(
                "pressure",
                "pump.pressure",
                PointAddress::modbus_holding_register(40001),
                TelemetryType::Float,
                "pressure",
            ))
            .with_point(DataConfigPoint::new(
                "running",
                "pump.running",
                PointAddress::modbus_holding_register(40002),
                TelemetryType::Boolean,
                "running",
            )),
        );

    let json = serde_json::to_value(&package).unwrap();
    assert_eq!(json["data_configs"][0]["config_id"], "pump_status");
    assert_eq!(json["data_configs"][0]["collection"]["period_ms"], 1000);
    assert_eq!(
        json["data_configs"][0]["publish"]["topic_template"],
        "factory/{site}/pump/{device_id}/status"
    );
    assert_eq!(
        json["data_configs"][0]["points"][0]["json_field"],
        "pressure"
    );
}

#[test]
fn mqtt5_connection_and_last_will_properties_round_trip_through_edge_config() {
    let mut uplink =
        MqttUplinkConfig::velamq("velamq-main", "mqtts://velamq.local:8883", "runtime-1");
    uplink.protocol_version = MqttProtocolVersion::V5_0;
    uplink.clean_start = false;
    uplink.session_expiry_interval_seconds = 3_600;
    uplink.receive_maximum = Some(32);
    uplink.maximum_packet_size_bytes = Some(1_048_576);
    uplink.topic_alias_maximum = Some(16);
    uplink.request_response_information = true;
    uplink.user_properties = vec![MqttUserProperty {
        key: "tenant".to_string(),
        value: "factory-a".to_string(),
    }];
    uplink.last_will = Some(MqttLastWillConfig {
        topic: "edge/edge-dev/status".to_string(),
        payload: r#"{"status":"offline"}"#.to_string(),
        qos: 1,
        retain: true,
        delay_interval_seconds: 10,
        payload_format_utf8: true,
        message_expiry_interval_seconds: 300,
        content_type: Some("application/json".to_string()),
        response_topic: Some("edge/edge-dev/status/ack".to_string()),
        correlation_data: Some("runtime-1".to_string()),
        user_properties: vec![MqttUserProperty {
            key: "reason".to_string(),
            value: "disconnect".to_string(),
        }],
    });

    let package = EdgeConfigPackage::new("edge-dev", "v1").with_mqtt_uplink(uplink);
    let json = serde_json::to_value(&package).expect("package serializes");
    assert_eq!(json["mqtt_uplinks"][0]["receive_maximum"], 32);
    assert_eq!(
        json["mqtt_uplinks"][0]["last_will"]["content_type"],
        "application/json"
    );

    let decoded: EdgeConfigPackage = serde_json::from_value(json).expect("package deserializes");
    let decoded_uplink = &decoded.mqtt_uplinks[0];
    assert_eq!(decoded_uplink.protocol_version, MqttProtocolVersion::V5_0);
    assert_eq!(decoded_uplink.topic_alias_maximum, Some(16));
    assert_eq!(
        decoded_uplink.last_will.as_ref().map(|will| will.qos),
        Some(1)
    );
    assert_eq!(
        decoded_uplink
            .last_will
            .as_ref()
            .and_then(|will| will.response_topic.as_deref()),
        Some("edge/edge-dev/status/ack")
    );
}

#[test]
fn algorithm_dsl_binds_point_inputs_and_virtual_outputs() {
    let algorithm = AlgorithmSpec::dsl(
        "pressure-window-summary",
        "v1",
        AlgorithmKind::WindowAggregate,
        AlgorithmDsl {
            inputs: vec![AlgorithmInputBinding::new("p", "pressure")],
            trigger: AlgorithmTrigger::window(60_000),
            steps: vec![AlgorithmStep::window_aggregate(
                "p",
                vec![WindowAggregateFunction::Avg {
                    output: "pressure_avg".to_string(),
                }],
            )],
            outputs: vec![AlgorithmOutput::virtual_point(
                "pressure_avg",
                "pressure.avg_1m",
            )],
            report: AlgorithmReportPolicy::new(AlgorithmReportMode::WindowResult, "velamq-main"),
        },
    );

    let package = EdgeConfigPackage::new("edge-dev", "2026.06.28-001")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-main"))
        .with_point_mapping(TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pressure",
            "sim-main",
            PointAddress::simulated("pressure"),
            TelemetryType::Float,
        ))
        .with_algorithm(algorithm.clone());

    assert_eq!(package.algorithms[0].kind, AlgorithmKind::WindowAggregate);
    assert_eq!(package.algorithms[0].inputs(), vec!["pressure"]);
    assert_eq!(package.algorithms[0].outputs(), vec!["pressure.avg_1m"]);

    let json = serde_json::to_value(&algorithm).expect("algorithm serializes");
    assert_eq!(json["kind"], "WindowAggregate");
    assert_eq!(json["dsl"]["inputs"][0]["pointId"], "pressure");
    assert_eq!(json["dsl"]["outputs"][0]["pointId"], "pressure.avg_1m");
}

#[test]
fn duration_condition_dsl_preserves_cloud_runtime_wire_contract() {
    let algorithm = AlgorithmSpec::dsl(
        "pressure-high-duration",
        "v1",
        AlgorithmKind::DurationRule,
        AlgorithmDsl {
            inputs: vec![AlgorithmInputBinding::new("p", "pressure")],
            trigger: AlgorithmTrigger::on_sample(),
            steps: vec![AlgorithmStep::DurationCondition {
                source: "p".to_string(),
                operator: CompareOperator::Gte,
                threshold: 10.0,
                duration_ms: 5_000,
                output: "value".to_string(),
            }],
            outputs: vec![AlgorithmOutput::virtual_point("value", "pressure.high_5s")],
            report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnOutput, "velamq-main"),
        },
    );

    let json = serde_json::to_value(&algorithm).expect("algorithm serializes");
    assert_eq!(json["kind"], "DurationRule");
    assert_eq!(json["dsl"]["steps"][0]["type"], "durationCondition");
    assert_eq!(json["dsl"]["steps"][0]["operator"], "Gte");
    assert_eq!(json["dsl"]["steps"][0]["durationMs"], 5_000);

    let decoded: AlgorithmSpec =
        serde_json::from_value(json).expect("algorithm deserializes from EdgeLink JSON");
    assert_eq!(decoded.kind, AlgorithmKind::DurationRule);
    assert!(matches!(
        decoded.dsl.steps.as_slice(),
        [AlgorithmStep::DurationCondition {
            duration_ms: 5_000,
            ..
        }]
    ));
}

#[test]
fn controlled_field_preflight_fixture_matches_the_runtime_config_contract() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/fixtures/field-preflight-config.json");
    let json = std::fs::read_to_string(&fixture).expect("preflight fixture is readable");
    let package: EdgeConfigPackage =
        serde_json::from_str(&json).expect("preflight fixture matches EdgeConfigPackage");

    assert_eq!(package.edge_id, "edge-preflight");
    assert_eq!(package.protocol_connections.len(), 1);
    assert_eq!(
        package.protocol_connections[0].protocol,
        ProtocolType::ModbusRtu
    );
    assert_eq!(package.data_configs.len(), 1);
    assert_eq!(package.data_configs[0].points.len(), 1);
    assert_eq!(package.mqtt_uplinks[0].qos, 1);
}

#[test]
fn legacy_algorithm_runtime_values_migrate_to_the_dsl_engine() {
    for legacy in ["Rule", "Wasm", "Onnx", "Python"] {
        let runtime: AlgorithmRuntime =
            serde_json::from_str(&format!("\"{legacy}\"")).expect("legacy value migrates");
        assert_eq!(runtime, AlgorithmRuntime::Rule);
        assert_eq!(serde_json::to_string(&runtime).unwrap(), "\"Rule\"");
    }

    assert!(serde_json::from_str::<AlgorithmRuntime>("\"NativePlugin\"").is_err());
}

#[test]
fn opc_ua_discovery_contract_is_bounded_and_wire_compatible() {
    let request = DiscoveryRequest::opc_ua_browse(
        "browse-opcua-main",
        "opcua-main",
        "ns=2;s=Factory/Line1",
        4,
    );
    request.validate().expect("OPC UA browse request is valid");
    assert_eq!(request.address_kind, DiscoveryAddressKind::OpcUaBrowse);
    assert_eq!(request.point_count().unwrap(), 0);

    let json = serde_json::to_value(&request).expect("request serializes");
    assert_eq!(json["address_kind"], "opc_ua_browse");
    assert_eq!(json["root_node_id"], "ns=2;s=Factory/Line1");
    assert_eq!(json["max_depth"], 4);
    assert!(!json["include_standard_namespace"].as_bool().unwrap());

    let decoded: DiscoveryRequest = serde_json::from_value(json).expect("request deserializes");
    assert_eq!(decoded, request);
}

#[test]
fn opc_ua_discovery_contract_rejects_invalid_root_and_depth() {
    let invalid_root =
        DiscoveryRequest::opc_ua_browse("browse-opcua-main", "opcua-main", "Factory/Line1", 4);
    assert!(invalid_root.validate().is_err());

    let unbounded = DiscoveryRequest::opc_ua_browse("browse-opcua-main", "opcua-main", "i=85", 9);
    assert!(unbounded.validate().is_err());
}

#[test]
fn writable_opc_ua_points_require_an_exact_compatible_write_type() {
    let address = PointAddress::opc_ua_node_id("ns=2;s=Pump/SpeedSetpoint");
    assert!(edge_core::validate_opc_ua_point(
        &address,
        TelemetryType::Integer,
        PointAccess::ReadWrite,
        None,
    )
    .unwrap_err()
    .contains("writeDataType"));
    assert!(edge_core::validate_opc_ua_point(
        &address,
        TelemetryType::Integer,
        PointAccess::ReadWrite,
        Some(OpcUaPointOptions::new(OpcUaWriteDataType::UInt16)),
    )
    .is_ok());
    assert!(edge_core::validate_opc_ua_point(
        &address,
        TelemetryType::Float,
        PointAccess::ReadWrite,
        Some(OpcUaPointOptions::new(OpcUaWriteDataType::UInt16)),
    )
    .unwrap_err()
    .contains("incompatible"));

    let mapping = TelemetryPointMapping::new(
        "speed_setpoint",
        "pump-1",
        "pump.speed_setpoint",
        "opcua-main",
        address,
        TelemetryType::Integer,
    )
    .with_access(PointAccess::ReadWrite)
    .with_opc_ua_options(OpcUaPointOptions::new(OpcUaWriteDataType::UInt16));
    let json = serde_json::to_value(mapping).expect("OPC UA mapping serializes");
    assert_eq!(json["opc_ua"]["writeDataType"], "UInt16");
}
