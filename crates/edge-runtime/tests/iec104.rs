use bytes::Bytes;
use edge_core::{
    CommandFlowConfig, CommandGraphEdge, CommandGraphNode, CommandGraphNodeKind, DeviceInstance,
    EdgeConfigPackage, Iec104ConnectionSettings, Iec104ControlType, Iec104PointOptions,
    MqttUplinkConfig, PointAccess, PointAddress, ProtocolConnection, TelemetryPointMapping,
    TelemetryType, TelemetryValue,
};
use edge_runtime::{
    CommandExecutionStatus, ConfiguredEdgeRuntime, Iec104Adapter, ProtocolAdapter,
    ProtocolCommandAdapter, TokioSerialBusFactory,
};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_util::codec::Framed;
use voltage_iec104::{
    Apci, Apdu, Asdu, AsduHeader, Cot, Cp56Time2a, Iec104Codec, InformationObject, Ioa, TypeId,
    UFunction,
};

#[tokio::test]
async fn iec104_adapter_collects_multiple_types_and_reuses_one_session() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("IEC 104 test listener binds");
    let endpoint = listener.local_addr().expect("listener has address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("IEC 104 client connects");
        let mut framed = Framed::new(stream, Iec104Codec::new());
        let start = framed
            .next()
            .await
            .expect("STARTDT request")
            .expect("valid STARTDT request");
        assert_eq!(
            start.apci,
            Apci::UFrame {
                function: UFunction::StartDtAct
            }
        );
        framed
            .send(Apdu::u_frame(UFunction::StartDtCon))
            .await
            .expect("STARTDT confirmation sends");

        let mut server_send_sequence = 0_u16;
        for client_send_sequence in 0_u16..2 {
            let interrogation = framed
                .next()
                .await
                .expect("interrogation request")
                .expect("valid interrogation request");
            assert_eq!(
                interrogation.apci,
                Apci::IFrame {
                    send_seq: client_send_sequence,
                    recv_seq: server_send_sequence,
                }
            );
            let request_asdu = interrogation.asdu.expect("interrogation contains ASDU");
            assert_eq!(request_asdu.header.type_id, TypeId::InterrogationCommand);
            assert_eq!(request_asdu.header.common_address, 1);

            let client_receive_sequence = client_send_sequence + 1;
            framed
                .send(Apdu::i_frame(
                    server_send_sequence,
                    client_receive_sequence,
                    float_asdu(1001, 12.5 + f32::from(client_send_sequence)),
                ))
                .await
                .expect("float data sends");
            server_send_sequence += 1;
            framed
                .send(Apdu::i_frame(
                    server_send_sequence,
                    client_receive_sequence,
                    single_point_asdu(1002, client_send_sequence == 1),
                ))
                .await
                .expect("single-point data sends");
            server_send_sequence += 1;
        }
    });

    let connection = ProtocolConnection::iec104("iec104-main", endpoint.to_string())
        .with_iec104_settings(
            Iec104ConnectionSettings::default().with_cp56_timezone_offset_minutes(8 * 60),
        );
    let mappings = vec![
        TelemetryPointMapping::new(
            "pressure",
            "pump-1",
            "pressure",
            "iec104-main",
            PointAddress::iec104(1, 1001),
            TelemetryType::Float,
        ),
        TelemetryPointMapping::new(
            "running",
            "pump-1",
            "running",
            "iec104-main",
            PointAddress::iec104(1, 1002),
            TelemetryType::Boolean,
        ),
    ];
    let mut adapter = Iec104Adapter::new(connection, mappings).expect("adapter builds");

    let first = adapter.read_telemetry().await.expect("first read succeeds");
    assert_eq!(first[0].value, TelemetryValue::Float(12.5));
    assert_eq!(first[1].value, TelemetryValue::Boolean(false));
    assert_eq!(
        first[0].timestamp.to_rfc3339(),
        "2026-07-18T04:34:56.789+00:00"
    );

    let second = adapter
        .read_telemetry()
        .await
        .expect("second read succeeds");
    assert_eq!(second[0].value, TelemetryValue::Float(13.5));
    assert_eq!(second[1].value, TelemetryValue::Boolean(true));
    assert_eq!(adapter.connection_generation(), 1);
    server.await.expect("IEC 104 test server completes");
}

#[tokio::test]
async fn iec104_adapter_executes_select_before_operate_over_a_real_tcp_session() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("IEC 104 command test listener binds");
    let endpoint = listener.local_addr().expect("listener has address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("IEC 104 client connects");
        let mut framed = Framed::new(stream, Iec104Codec::new());
        let start = framed
            .next()
            .await
            .expect("STARTDT request")
            .expect("valid STARTDT request");
        assert_eq!(
            start.apci,
            Apci::UFrame {
                function: UFunction::StartDtAct
            }
        );
        framed
            .send(Apdu::u_frame(UFunction::StartDtCon))
            .await
            .expect("STARTDT confirmation sends");

        let select = next_i_frame(&mut framed).await;
        assert_command(&select, 0, 0, TypeId::SingleCommand, 7, 1201, &[0x81]);
        framed
            .send(Apdu::i_frame(
                0,
                1,
                command_confirmation(TypeId::SingleCommand, 7, 1201, &[0x81]),
            ))
            .await
            .expect("select confirmation sends");

        let execute = next_i_frame(&mut framed).await;
        assert_command(&execute, 1, 1, TypeId::SingleCommand, 7, 1201, &[0x01]);
        framed
            .send(Apdu::i_frame(
                1,
                2,
                command_confirmation(TypeId::SingleCommand, 7, 1201, &[0x01]),
            ))
            .await
            .expect("execute confirmation sends");
    });

    let connection = ProtocolConnection::iec104("iec104-command", endpoint.to_string());
    let mapping = TelemetryPointMapping::new(
        "breaker_close",
        "substation-1",
        "breaker.close",
        "iec104-command",
        PointAddress::iec104(7, 1201),
        TelemetryType::Boolean,
    )
    .with_access(PointAccess::ReadWrite)
    .with_iec104_options(
        Iec104PointOptions::new(Iec104ControlType::SingleCommand).with_select_before_operate(true),
    );
    let mut adapter = Iec104Adapter::new(connection, vec![mapping.clone()])
        .expect("IEC 104 command adapter builds");
    let result = adapter
        .write_point(&mapping, TelemetryValue::Boolean(true))
        .await
        .expect("IEC 104 SBO command succeeds");

    assert_eq!(result.point_id, "breaker_close");
    assert_eq!(result.value, TelemetryValue::Boolean(true));
    assert!(result.verified);
    assert_eq!(result.readback_value, None);
    assert_eq!(adapter.connection_generation(), 1);
    server.await.expect("IEC 104 command server completes");
}

#[tokio::test]
async fn iec104_adapter_executes_double_and_float_commands_over_one_real_tcp_session() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("IEC 104 command test listener binds");
    let endpoint = listener.local_addr().expect("listener has address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("IEC 104 client connects");
        let mut framed = Framed::new(stream, Iec104Codec::new());
        let start = framed
            .next()
            .await
            .expect("STARTDT request")
            .expect("valid STARTDT request");
        assert_eq!(
            start.apci,
            Apci::UFrame {
                function: UFunction::StartDtAct
            }
        );
        framed
            .send(Apdu::u_frame(UFunction::StartDtCon))
            .await
            .expect("STARTDT confirmation sends");

        let double = next_i_frame(&mut framed).await;
        assert_command(&double, 0, 0, TypeId::DoubleCommand, 7, 1202, &[0x02]);
        framed
            .send(Apdu::i_frame(
                0,
                1,
                command_confirmation(TypeId::DoubleCommand, 7, 1202, &[0x02]),
            ))
            .await
            .expect("double-command confirmation sends");

        let mut float_information = 42.5_f32.to_le_bytes().to_vec();
        float_information.push(0);
        let setpoint = next_i_frame(&mut framed).await;
        assert_command(
            &setpoint,
            1,
            1,
            TypeId::SetpointFloat,
            7,
            1203,
            &float_information,
        );
        framed
            .send(Apdu::i_frame(
                1,
                2,
                command_confirmation(TypeId::SetpointFloat, 7, 1203, &float_information),
            ))
            .await
            .expect("float-setpoint confirmation sends");
    });

    let connection = ProtocolConnection::iec104("iec104-command", endpoint.to_string());
    let double_mapping = TelemetryPointMapping::new(
        "breaker_state",
        "substation-1",
        "breaker.state",
        "iec104-command",
        PointAddress::iec104(7, 1202),
        TelemetryType::Integer,
    )
    .with_access(PointAccess::ReadWrite)
    .with_iec104_options(Iec104PointOptions::new(Iec104ControlType::DoubleCommand));
    let float_mapping = TelemetryPointMapping::new(
        "active_power_setpoint",
        "substation-1",
        "active_power.setpoint",
        "iec104-command",
        PointAddress::iec104(7, 1203),
        TelemetryType::Float,
    )
    .with_access(PointAccess::ReadWrite)
    .with_iec104_options(Iec104PointOptions::new(Iec104ControlType::SetpointFloat));
    let mut adapter = Iec104Adapter::new(
        connection,
        vec![double_mapping.clone(), float_mapping.clone()],
    )
    .expect("IEC 104 command adapter builds");

    let double_result = adapter
        .write_point(&double_mapping, TelemetryValue::Integer(2))
        .await
        .expect("IEC 104 double command succeeds");
    let float_result = adapter
        .write_point(&float_mapping, TelemetryValue::Float(42.5))
        .await
        .expect("IEC 104 float setpoint succeeds");

    assert_eq!(double_result.value, TelemetryValue::Integer(2));
    assert!(double_result.verified);
    assert_eq!(float_result.value, TelemetryValue::Float(42.5));
    assert!(float_result.verified);
    assert_eq!(adapter.connection_generation(), 1);
    server.await.expect("IEC 104 command server completes");
}

#[tokio::test]
async fn configured_runtime_executes_an_iec104_mqtt_command_flow() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("IEC 104 runtime command listener binds");
    let endpoint = listener.local_addr().expect("listener has address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("IEC 104 runtime connects");
        let mut framed = Framed::new(stream, Iec104Codec::new());
        let start = framed
            .next()
            .await
            .expect("STARTDT request")
            .expect("valid STARTDT request");
        assert!(matches!(
            start.apci,
            Apci::UFrame {
                function: UFunction::StartDtAct
            }
        ));
        framed
            .send(Apdu::u_frame(UFunction::StartDtCon))
            .await
            .expect("STARTDT confirmation sends");

        let execute = next_i_frame(&mut framed).await;
        assert_command(&execute, 0, 0, TypeId::SingleCommand, 7, 1201, &[0x01]);
        framed
            .send(Apdu::i_frame(
                0,
                1,
                command_confirmation(TypeId::SingleCommand, 7, 1201, &[0x01]),
            ))
            .await
            .expect("execute confirmation sends");
    });

    let mapping = TelemetryPointMapping::new(
        "breaker_close",
        "substation-1",
        "breaker.close",
        "iec104-main",
        PointAddress::iec104(7, 1201),
        TelemetryType::Boolean,
    )
    .with_access(PointAccess::ReadWrite)
    .with_iec104_options(Iec104PointOptions::new(Iec104ControlType::SingleCommand));
    let package = EdgeConfigPackage::new("edge-iec104-command", "v1")
        .with_device(DeviceInstance::new("substation-1", "substation"))
        .with_protocol_connection(ProtocolConnection::iec104(
            "iec104-main",
            endpoint.to_string(),
        ))
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://127.0.0.1:1883",
            "iec104-command-test",
        ))
        .with_point_mapping(mapping)
        .with_command_flow(iec104_write_flow());
    let mut runtime = ConfiguredEdgeRuntime::new(package, TokioSerialBusFactory)
        .expect("runtime accepts IEC 104 command package");

    let report = runtime
        .execute_command_flow_message(
            "close-breaker",
            br#"{"commandId":"cmd-iec104-close","value":true}"#,
        )
        .await
        .expect("IEC 104 command flow executes");

    assert_eq!(report.status, CommandExecutionStatus::Succeeded);
    assert_eq!(report.writes.len(), 1);
    assert!(report.writes[0].verified);
    assert_eq!(report.writes[0].value, TelemetryValue::Boolean(true));
    assert_eq!(
        runtime
            .shadow("substation-1")
            .and_then(|shadow| shadow.latest_value("breaker_close")),
        Some(&TelemetryValue::Boolean(true))
    );
    let metrics = runtime.protocol_runtime_metrics();
    assert!(metrics[0].connected);
    assert_eq!(metrics[0].error_count, 0);
    assert_eq!(metrics[0].write_attempt_count, 1);
    assert_eq!(metrics[0].write_success_count, 1);
    server.await.expect("IEC 104 runtime server completes");
}

#[tokio::test]
async fn iec104_adapter_rejects_an_invalid_double_command_before_connecting() {
    let connection = ProtocolConnection::iec104("iec104-command", "127.0.0.1:1");
    let mapping = TelemetryPointMapping::new(
        "breaker_state",
        "substation-1",
        "breaker.state",
        "iec104-command",
        PointAddress::iec104(7, 1202),
        TelemetryType::Integer,
    )
    .with_access(PointAccess::ReadWrite)
    .with_iec104_options(Iec104PointOptions::new(Iec104ControlType::DoubleCommand));
    let mut adapter = Iec104Adapter::new(connection, vec![mapping.clone()])
        .expect("IEC 104 double-command adapter builds");

    let error = adapter
        .write_point(&mapping, TelemetryValue::Integer(3))
        .await
        .expect_err("reserved double-command values are rejected");

    assert!(error.to_string().contains("must be 1 (OFF) or 2 (ON)"));
    assert_eq!(adapter.connection_generation(), 0);
}

#[test]
fn iec104_adapter_requires_one_station_per_connection() {
    let connection = ProtocolConnection::iec104("iec104-main", "127.0.0.1:2404");
    let mappings = vec![
        TelemetryPointMapping::new(
            "p1",
            "station-1",
            "p1",
            "iec104-main",
            PointAddress::iec104(1, 1001),
            TelemetryType::Float,
        ),
        TelemetryPointMapping::new(
            "p2",
            "station-2",
            "p2",
            "iec104-main",
            PointAddress::iec104(2, 1002),
            TelemetryType::Float,
        ),
    ];
    let error = Iec104Adapter::new(connection, mappings)
        .err()
        .expect("mixed common addresses are rejected");
    assert!(error.to_string().contains("one common address"));
}

fn float_asdu(ioa: u32, value: f32) -> Asdu {
    let mut data = value.to_le_bytes().to_vec();
    data.push(0);
    data.extend(
        Cp56Time2a {
            milliseconds: 56_789,
            minutes: 34,
            hours: 12,
            day: 18,
            day_of_week: 6,
            month: 7,
            year: 26,
            invalid: false,
            summer_time: false,
        }
        .to_bytes(),
    );
    asdu(TypeId::MeasuredFloatTime56, ioa, Bytes::from(data))
}

fn single_point_asdu(ioa: u32, value: bool) -> Asdu {
    asdu(TypeId::SinglePoint, ioa, Bytes::from(vec![u8::from(value)]))
}

fn asdu(type_id: TypeId, ioa: u32, data: Bytes) -> Asdu {
    let mut asdu = Asdu::new(AsduHeader::new(type_id, 1, Cot::InterrogatedByStation, 1));
    asdu.objects
        .push(InformationObject::new(Ioa::new(ioa), data));
    asdu
}

async fn next_i_frame(framed: &mut Framed<tokio::net::TcpStream, Iec104Codec>) -> Apdu {
    loop {
        let frame = framed
            .next()
            .await
            .expect("IEC 104 command frame")
            .expect("valid IEC 104 command frame");
        if matches!(frame.apci, Apci::IFrame { .. }) {
            return frame;
        }
    }
}

fn assert_command(
    apdu: &Apdu,
    send_seq: u16,
    recv_seq: u16,
    type_id: TypeId,
    common_address: u16,
    ioa: u32,
    information: &[u8],
) {
    assert_eq!(apdu.apci, Apci::IFrame { send_seq, recv_seq });
    let asdu = apdu.asdu.as_ref().expect("command contains ASDU");
    assert_eq!(asdu.header.type_id, type_id);
    assert_eq!(asdu.header.cot, Cot::Activation);
    assert_eq!(asdu.header.common_address, common_address);
    assert_eq!(&asdu.raw_data[..3], &Ioa::new(ioa).to_bytes());
    assert_eq!(&asdu.raw_data[3..], information);
}

fn command_confirmation(
    type_id: TypeId,
    common_address: u16,
    ioa: u32,
    information: &[u8],
) -> Asdu {
    let mut asdu = Asdu::new(AsduHeader::new(
        type_id,
        1,
        Cot::ActivationConfirm,
        common_address,
    ));
    asdu.objects.push(InformationObject::new(
        Ioa::new(ioa),
        Bytes::copy_from_slice(information),
    ));
    asdu
}

fn iec104_write_flow() -> CommandFlowConfig {
    CommandFlowConfig::new(
        "close-breaker",
        "合闸",
        "velamq-main",
        "factory/{edge_id}/command/breaker",
        "factory/{edge_id}/reply/{command_id}",
    )
    .with_node(CommandGraphNode::new(
        "input",
        CommandGraphNodeKind::MqttInput,
        "MQTT 输入",
    ))
    .with_node(
        CommandGraphNode::new(
            "write-breaker",
            CommandGraphNodeKind::PointWrite,
            "写入断路器",
        )
        .with_ref("breaker_close"),
    )
    .with_node(CommandGraphNode::new(
        "reply",
        CommandGraphNodeKind::MqttReply,
        "MQTT 回执",
    ))
    .with_edge(CommandGraphEdge::new(
        "input-write",
        "input",
        "write-breaker",
    ))
    .with_edge(CommandGraphEdge::new(
        "write-reply",
        "write-breaker",
        "reply",
    ))
}
