use std::collections::BTreeMap;

use edge_core::{
    CollectionTask, CommandFlowConfig, CommandGraphEdge, CommandGraphNode, CommandGraphNodeKind,
    DataConfig, DataConfigCollection, DataConfigGraphEdge, DataConfigGraphNode,
    DataConfigGraphNodeKind, DataConfigPayload, DataConfigPoint, DataConfigPublish,
    DataConfigVisualGraph, DeviceInstance, EdgeConfigPackage, MqttUplinkConfig,
    OmronFinsConnectionSettings, OmronFinsTransport, OmronFinsWordOrder, PointAccess, PointAddress,
    ProtocolConnection, ProtocolType, SiemensS7ConnectionSettings, TelemetryPointMapping,
    TelemetryType,
};

use crate::{PointSet, PointSetPoint, Product, ProductVersion, ProductVersionStatus};

pub const SIEMENS_S7_TEMPLATE_ID: &str = "siemens-s7-pump-basic";
pub const OMRON_FINS_TEMPLATE_ID: &str = "omron-fins-machine-basic";

#[derive(Clone, Debug, PartialEq)]
pub struct ProductTemplateBundle {
    pub point_set: PointSet,
    pub product: Product,
    pub version: ProductVersion,
}

impl ProductTemplateBundle {
    pub fn materialize(&self, edge_id: &str) -> EdgeConfigPackage {
        let connection_id = self.version.protocol_connections[0].connection_id.clone();
        let device_id = self.version.devices[0].device_id.clone();
        let point_mappings = self
            .point_set
            .points
            .iter()
            .map(|point| {
                let mut mapping = TelemetryPointMapping::new(
                    point.point_id.clone(),
                    device_id.clone(),
                    point.semantic_id.clone(),
                    connection_id.clone(),
                    point.address.clone(),
                    point.value_type,
                )
                .with_access(point.access)
                .with_interval_ms(point.interval_ms);
                mapping.bacnet = point.bacnet;
                mapping.unit = point.unit.clone();
                mapping
            })
            .collect();
        let mqtt_uplinks = self
            .version
            .mqtt_uplinks
            .iter()
            .cloned()
            .map(|mut uplink| {
                uplink.client_id = uplink.client_id.replace("{edge_id}", edge_id);
                uplink
            })
            .collect();

        EdgeConfigPackage {
            edge_id: edge_id.to_string(),
            version: self.version.version.clone(),
            device_models: self.version.device_models.clone(),
            devices: self.version.devices.clone(),
            protocol_connections: self.version.protocol_connections.clone(),
            mqtt_uplinks,
            data_configs: self.version.data_configs.clone(),
            command_flows: self.version.command_flows.clone(),
            point_mappings,
            collection_tasks: self.version.collection_tasks.clone(),
            algorithms: self.version.algorithms.clone(),
        }
    }
}

pub fn manufacturer_product_templates(project_id: &str) -> Vec<ProductTemplateBundle> {
    vec![
        siemens_s7_product_template(project_id),
        omron_fins_product_template(project_id),
    ]
}

fn siemens_s7_product_template(project_id: &str) -> ProductTemplateBundle {
    let point_set = point_set(
        "siemens-s7-pump-points",
        project_id,
        "Siemens S7 泵站点位集",
        "S7 数据块中的压力、运行状态、转速及可写启停命令。",
        ProtocolType::SiemensS7,
        vec![
            point(
                "s7_pressure",
                "pump.pressure",
                PointAddress::siemens_s7("DB1.REAL0"),
                TelemetryType::Float,
                PointAccess::ReadOnly,
                Some("MPa"),
                1_000,
            ),
            point(
                "s7_running",
                "pump.running",
                PointAddress::siemens_s7("DB1.DBX4.0"),
                TelemetryType::Boolean,
                PointAccess::ReadOnly,
                None,
                1_000,
            ),
            point(
                "s7_speed",
                "pump.speed",
                PointAddress::siemens_s7("DB1.DINT6"),
                TelemetryType::Integer,
                PointAccess::ReadOnly,
                Some("rpm"),
                1_000,
            ),
            point(
                "s7_start_command",
                "pump.command.start",
                PointAddress::siemens_s7("DB1.DBX10.0"),
                TelemetryType::Boolean,
                PointAccess::ReadWrite,
                None,
                1_000,
            ),
        ],
    );
    let connection = ProtocolConnection::siemens_s7(
        "siemens-s7-main",
        "s7://192.168.0.10:102",
        SiemensS7ConnectionSettings::default(),
    );
    product_template(
        SIEMENS_S7_TEMPLATE_ID,
        project_id,
        "Siemens S7 泵站标准模板",
        "siemens-s7-pump",
        "v1.0.0",
        point_set,
        "s7-pump-1",
        connection,
        "factory/{edge_id}/siemens/{device_id}/telemetry",
        "factory/{edge_id}/siemens/{device_id}/command",
        "s7_start_command",
    )
}

fn omron_fins_product_template(project_id: &str) -> ProductTemplateBundle {
    let point_set = point_set(
        "omron-fins-machine-points",
        project_id,
        "Omron FINS 机台点位集",
        "FINS 数据存储区中的计数、温度、运行状态及可写启停命令。",
        ProtocolType::OmronFins,
        vec![
            point(
                "fins_counter",
                "machine.counter",
                PointAddress::omron_fins("D100"),
                TelemetryType::Integer,
                PointAccess::ReadOnly,
                None,
                1_000,
            ),
            point(
                "fins_temperature",
                "machine.temperature",
                PointAddress::omron_fins("D102"),
                TelemetryType::Float,
                PointAccess::ReadOnly,
                Some("C"),
                1_000,
            ),
            point(
                "fins_running",
                "machine.running",
                PointAddress::omron_fins("CIO0.0"),
                TelemetryType::Boolean,
                PointAccess::ReadOnly,
                None,
                1_000,
            ),
            point(
                "fins_start_command",
                "machine.command.start",
                PointAddress::omron_fins("CIO0.1"),
                TelemetryType::Boolean,
                PointAccess::ReadWrite,
                None,
                1_000,
            ),
        ],
    );
    let connection = ProtocolConnection::omron_fins(
        "omron-fins-main",
        "fins://192.168.0.20:9600",
        OmronFinsConnectionSettings {
            transport: OmronFinsTransport::Tcp,
            source_node: 0,
            destination_node: 0,
            word_order: OmronFinsWordOrder::LowWordFirst,
            ..Default::default()
        },
    );
    product_template(
        OMRON_FINS_TEMPLATE_ID,
        project_id,
        "Omron FINS 机台标准模板",
        "omron-fins-machine",
        "v1.0.0",
        point_set,
        "fins-machine-1",
        connection,
        "factory/{edge_id}/omron/{device_id}/telemetry",
        "factory/{edge_id}/omron/{device_id}/command",
        "fins_start_command",
    )
}

#[allow(clippy::too_many_arguments)]
fn product_template(
    product_id: &str,
    project_id: &str,
    name: &str,
    product_type: &str,
    version_name: &str,
    point_set: PointSet,
    device_id: &str,
    connection: ProtocolConnection,
    telemetry_topic: &str,
    command_topic: &str,
    writable_point_id: &str,
) -> ProductTemplateBundle {
    let connection_id = connection.connection_id.clone();
    let readable_point_ids = point_set
        .points
        .iter()
        .filter(|point| point.access.is_readable())
        .map(|point| point.point_id.clone())
        .collect::<Vec<_>>();
    let data_config = data_config(
        product_id,
        device_id,
        &connection_id,
        telemetry_topic,
        &point_set.points,
    );
    let mut mqtt = MqttUplinkConfig::velamq(
        "velamq-main",
        "mqtts://velamq.local:8883",
        format!("{{edge_id}}-{product_id}"),
    )
    .with_topic_template(telemetry_topic);
    mqtt.batch_size = 100;
    mqtt.flush_interval_ms = 1_000;

    let mut product = Product::new(product_id, project_id, name, product_type);
    product.description = format!("{name}，包含可直接下发的采集与指令编排。");
    product.latest_version = Some(version_name.to_string());

    let mut version = ProductVersion::draft(product_id, version_name);
    version.status = ProductVersionStatus::Published;
    version.point_set_ids = vec![point_set.point_set_id.clone()];
    version.devices = vec![DeviceInstance::new(device_id, product_type)];
    version.protocol_connections = vec![connection];
    version.collection_tasks = vec![CollectionTask::interval(
        format!("{product_id}-scan"),
        device_id,
        readable_point_ids,
        1_000,
    )];
    version.data_configs = vec![data_config];
    version.command_flows = vec![command_flow(product_id, command_topic, writable_point_id)];
    version.mqtt_uplinks = vec![mqtt];

    ProductTemplateBundle {
        point_set,
        product,
        version,
    }
}

fn point_set(
    point_set_id: &str,
    project_id: &str,
    name: &str,
    description: &str,
    protocol: ProtocolType,
    points: Vec<PointSetPoint>,
) -> PointSet {
    let mut point_set = PointSet::new(point_set_id, project_id, name, protocol);
    point_set.description = description.to_string();
    point_set.points = points;
    point_set
}

#[allow(clippy::too_many_arguments)]
fn point(
    point_id: &str,
    semantic_id: &str,
    address: PointAddress,
    value_type: TelemetryType,
    access: PointAccess,
    unit: Option<&str>,
    interval_ms: u64,
) -> PointSetPoint {
    PointSetPoint {
        point_id: point_id.to_string(),
        semantic_id: semantic_id.to_string(),
        address,
        value_type,
        access,
        opc_ua: None,
        iec101: None,
        iec104: None,
        bacnet: None,
        unit: unit.map(str::to_string),
        interval_ms,
    }
}

fn data_config(
    product_id: &str,
    device_id: &str,
    connection_id: &str,
    telemetry_topic: &str,
    points: &[PointSetPoint],
) -> DataConfig {
    let readable_points = points
        .iter()
        .filter(|point| point.access.is_readable())
        .collect::<Vec<_>>();
    let mut config = DataConfig::new(
        format!("{product_id}-telemetry"),
        "实时遥测上报",
        device_id,
        connection_id,
        DataConfigCollection::new(1_000),
        DataConfigPublish::new("velamq-main", telemetry_topic, DataConfigPayload::object()),
    );
    config.points = readable_points
        .iter()
        .map(|point| {
            let mut data_point = DataConfigPoint::new(
                point.point_id.clone(),
                point.semantic_id.clone(),
                point.address.clone(),
                point.value_type,
                point.point_id.clone(),
            );
            data_point.unit = point.unit.clone();
            data_point
        })
        .collect();

    let mut nodes = readable_points
        .iter()
        .enumerate()
        .map(|(index, point)| DataConfigGraphNode {
            node_id: format!("point-{}", point.point_id),
            kind: DataConfigGraphNodeKind::Point,
            label: point.point_id.clone(),
            ref_id: Some(point.point_id.clone()),
            params: BTreeMap::new(),
            x: 60,
            y: 60 + (index as i32 * 90),
        })
        .collect::<Vec<_>>();
    nodes.push(DataConfigGraphNode {
        node_id: "json-package".to_string(),
        kind: DataConfigGraphNodeKind::Json,
        label: "JSON 组包".to_string(),
        ref_id: None,
        params: BTreeMap::new(),
        x: 360,
        y: 150,
    });
    nodes.push(DataConfigGraphNode {
        node_id: "mqtt-output".to_string(),
        kind: DataConfigGraphNodeKind::Mqtt,
        label: telemetry_topic.to_string(),
        ref_id: Some(telemetry_topic.to_string()),
        params: BTreeMap::new(),
        x: 650,
        y: 150,
    });
    let mut edges = readable_points
        .iter()
        .map(|point| DataConfigGraphEdge {
            edge_id: format!("point-{}-json", point.point_id),
            from: format!("point-{}", point.point_id),
            from_port: Some("output".to_string()),
            to: "json-package".to_string(),
            to_port: Some("input".to_string()),
        })
        .collect::<Vec<_>>();
    edges.push(DataConfigGraphEdge {
        edge_id: "json-mqtt".to_string(),
        from: "json-package".to_string(),
        from_port: Some("output".to_string()),
        to: "mqtt-output".to_string(),
        to_port: Some("input".to_string()),
    });
    config.visual_graph = DataConfigVisualGraph { nodes, edges };
    config
}

fn command_flow(
    product_id: &str,
    command_topic: &str,
    writable_point_id: &str,
) -> CommandFlowConfig {
    let mut safety_params = BTreeMap::new();
    safety_params.insert("max_commands".to_string(), serde_json::json!(10));
    safety_params.insert("window_ms".to_string(), serde_json::json!(60_000));
    safety_params.insert("require_confirmation".to_string(), serde_json::json!(true));
    let mut write_params = BTreeMap::new();
    write_params.insert(
        "value_path".to_string(),
        serde_json::json!(format!("values.{writable_point_id}")),
    );
    write_params.insert("verification".to_string(), serde_json::json!("readback"));

    let mut flow = CommandFlowConfig::new(
        format!("{product_id}-commands"),
        "设备指令下发",
        "velamq-main",
        command_topic,
        "factory/{edge_id}/command/reply/{command_id}",
    );
    flow.nodes = vec![
        command_node(
            "input",
            CommandGraphNodeKind::MqttInput,
            "MQTT 指令输入",
            40,
            120,
        ),
        command_node(
            "parse",
            CommandGraphNodeKind::JsonParse,
            "解析 JSON",
            250,
            120,
        ),
        CommandGraphNode {
            params: safety_params,
            ..command_node(
                "safety",
                CommandGraphNodeKind::SafetyGate,
                "安全策略",
                460,
                120,
            )
        },
        CommandGraphNode {
            ref_id: Some(writable_point_id.to_string()),
            params: write_params,
            ..command_node(
                "write",
                CommandGraphNodeKind::PointWrite,
                "写入点位",
                670,
                120,
            )
        },
        command_node(
            "reply",
            CommandGraphNodeKind::MqttReply,
            "MQTT 执行回执",
            880,
            120,
        ),
    ];
    flow.edges = vec![
        CommandGraphEdge::new("input-parse", "input", "parse"),
        CommandGraphEdge::new("parse-safety", "parse", "safety"),
        CommandGraphEdge::new("safety-write", "safety", "write"),
        CommandGraphEdge::new("write-reply", "write", "reply"),
    ];
    flow
}

fn command_node(
    node_id: &str,
    kind: CommandGraphNodeKind,
    label: &str,
    x: i32,
    y: i32,
) -> CommandGraphNode {
    CommandGraphNode {
        node_id: node_id.to_string(),
        kind,
        label: label.to_string(),
        ref_id: None,
        params: BTreeMap::new(),
        x,
        y,
    }
}
