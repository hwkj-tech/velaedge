use std::{sync::Arc, time::Duration};

use edge_core::{
    CommandFlowConfig, CommandGraphEdge, CommandGraphNode, CommandGraphNodeKind, DeviceInstance,
    EdgeConfigPackage, MqttUplinkConfig, PointAccess, PointAddress, ProtocolConnection,
    TelemetryPointMapping, TelemetryType, TelemetryValue,
};
use edge_runtime::{
    CommandRuntimeService, ModbusTcpAdapter, ProtocolAdapter, ProtocolCircuitBreakerRegistry,
    RocksEdgeRuntimeStore,
};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tempfile::tempdir;

#[tokio::test]
#[ignore = "requires a real MQTT broker and the Docker Modbus device"]
async fn mqtt_command_writes_docker_modbus_and_publishes_reply() {
    let broker_host =
        std::env::var("VELAEDGE_COMMAND_MQTT_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let broker_port = std::env::var("VELAEDGE_COMMAND_MQTT_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1883);
    let modbus_endpoint = std::env::var("VELAEDGE_COMMAND_MODBUS_ENDPOINT")
        .unwrap_or_else(|_| "127.0.0.1:1502".to_string());
    let run_id = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let edge_id = format!("command-acceptance-{run_id}");
    let command_topic_template = "velaedge/{edge_id}/commands/setpoint";
    let command_topic = command_topic_template.replace("{edge_id}", &edge_id);
    let reply_topic = format!("velaedge/{edge_id}/replies");
    let point = TelemetryPointMapping::new(
        "remote_setpoint",
        "docker-modbus",
        "device.remote_setpoint",
        "modbus-command",
        PointAddress::modbus_holding_register(40021),
        TelemetryType::Integer,
    )
    .with_access(PointAccess::ReadWrite);
    let mut write = CommandGraphNode::new(
        "write-setpoint",
        CommandGraphNodeKind::PointWrite,
        "写入远程设定值",
    )
    .with_ref("remote_setpoint");
    write.params.insert(
        "value_path".to_string(),
        serde_json::json!("payload.control.setpoint"),
    );
    write
        .params
        .insert("verification".to_string(), serde_json::json!("readback"));
    let flow = CommandFlowConfig::new(
        "set-docker-modbus",
        "Docker Modbus 设定值",
        "command-broker",
        command_topic_template,
        &reply_topic,
    )
    .with_node(CommandGraphNode::new(
        "mqtt-input",
        CommandGraphNodeKind::MqttInput,
        "MQTT 输入",
    ))
    .with_node(CommandGraphNode::new(
        "json-parse",
        CommandGraphNodeKind::JsonParse,
        "解析 JSON",
    ))
    .with_node(write)
    .with_node(CommandGraphNode::new(
        "mqtt-reply",
        CommandGraphNodeKind::MqttReply,
        "MQTT 回执",
    ))
    .with_edge(CommandGraphEdge::new(
        "input-parse",
        "mqtt-input",
        "json-parse",
    ))
    .with_edge(CommandGraphEdge::new(
        "parse-write",
        "json-parse",
        "write-setpoint",
    ))
    .with_edge(CommandGraphEdge::new(
        "write-reply",
        "write-setpoint",
        "mqtt-reply",
    ));
    let broker = format!("mqtt://{broker_host}:{broker_port}");
    let package = EdgeConfigPackage::new(&edge_id, "command-acceptance-v1")
        .with_device(DeviceInstance::new("docker-modbus", "modbus-device"))
        .with_protocol_connection(ProtocolConnection::modbus_tcp(
            "modbus-command",
            &modbus_endpoint,
        ))
        .with_point_mapping(point.clone())
        .with_mqtt_uplink(MqttUplinkConfig::velamq(
            "command-broker",
            &broker,
            format!("velaedge-command-runtime-{run_id}"),
        ))
        .with_command_flow(flow);
    let directory = tempdir().unwrap();
    let store =
        Arc::new(RocksEdgeRuntimeStore::open(directory.path().join("runtime.rocksdb")).unwrap());
    let service =
        CommandRuntimeService::start(package, store, ProtocolCircuitBreakerRegistry::default())
            .await
            .unwrap();

    let mut options = MqttOptions::new(
        format!("velaedge-command-probe-{run_id}"),
        broker_host,
        broker_port,
    );
    options.set_keep_alive(Duration::from_secs(10));
    let (client, mut eventloop) = AsyncClient::new(options, 20);
    client
        .subscribe(&reply_topic, QoS::AtLeastOnce)
        .await
        .unwrap();
    wait_for_subscription(&mut eventloop).await;
    tokio::time::sleep(Duration::from_millis(250)).await;

    client
        .publish(
            &command_topic,
            QoS::AtLeastOnce,
            false,
            br#"{"commandId":"cmd-docker-modbus","payload":{"control":{"setpoint":4321}}}"#,
        )
        .await
        .unwrap();
    let reply = wait_for_reply(&mut eventloop, &reply_topic).await;
    assert_eq!(reply["commandId"], "cmd-docker-modbus");
    assert_eq!(reply["status"], "succeeded");
    assert_eq!(reply["writes"][0]["pointId"], "remote_setpoint");
    assert_eq!(reply["writes"][0]["verified"], true);

    let connection = ProtocolConnection::modbus_tcp("modbus-command", modbus_endpoint);
    let mut adapter = ModbusTcpAdapter::new(connection, vec![point]);
    let samples = adapter.read_telemetry().await.unwrap();
    assert_eq!(samples[0].value, TelemetryValue::Integer(4321));
    assert_eq!(service.enabled_flow_count(), 1);
}

async fn wait_for_subscription(eventloop: &mut rumqttc::EventLoop) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(
                eventloop.poll().await.unwrap(),
                Event::Incoming(Packet::SubAck(_))
            ) {
                break;
            }
        }
    })
    .await
    .expect("MQTT subscription acknowledgement timed out");
}

async fn wait_for_reply(
    eventloop: &mut rumqttc::EventLoop,
    reply_topic: &str,
) -> serde_json::Value {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if let Event::Incoming(Packet::Publish(publish)) = eventloop.poll().await.unwrap() {
                if publish.topic == reply_topic {
                    return serde_json::from_slice(&publish.payload).unwrap();
                }
            }
        }
    })
    .await
    .expect("MQTT command reply timed out")
}
