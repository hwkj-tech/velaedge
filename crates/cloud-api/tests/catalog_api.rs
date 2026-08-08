use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use cloud_api::{app, AppState, BootstrapMode};
use cloud_control::{
    EdgeNode, PointSet, PointSetPoint, Product, ProductVersion, ProductVersionStatus, Project,
    ReleaseStatus, SqliteCloudStore, OMRON_FINS_TEMPLATE_ID, SIEMENS_S7_TEMPLATE_ID,
};
use edge_core::{
    CommandFlowConfig, CommandGraphEdge, CommandGraphNode, CommandGraphNodeKind, DataConfig,
    DataConfigCollection, DataConfigGraphEdge, DataConfigGraphNode, DataConfigGraphNodeKind,
    DataConfigPayload, DataConfigPoint, DataConfigPublish, DeviceInstance,
    Iec101ConnectionSettings, Iec101ControlType, Iec101PointOptions, Iec104ConnectionSettings,
    Iec104ControlType, Iec104PointOptions, MqttUplinkConfig, OpcUaConnectionSettings,
    OpcUaPointOptions, OpcUaWriteDataType, PointAccess, PointAddress, ProtocolConnection,
    ProtocolType, SerialConnectionSettings, TelemetryType,
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn send_json(router: Router, method: &str, uri: &str, payload: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap()
    };
    (status, payload)
}

async fn get_json(router: Router, uri: &str) -> (StatusCode, Value) {
    let response = router
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn runtime_protocol_catalog_is_available_as_the_cloud_configuration_contract() {
    let (status, payload) = get_json(app(AppState::default()), "/api/protocols/catalog").await;

    assert_eq!(status, StatusCode::OK);
    let protocols = payload.as_array().unwrap();
    assert_eq!(protocols.len(), 11);
    assert!(protocols.iter().any(|protocol| {
        protocol["protocolType"] == "SiemensS7"
            && protocol["capabilityId"] == "siemens-s7"
            && protocol["commandWrite"] == true
    }));
    assert!(protocols.iter().any(|protocol| {
        protocol["protocolType"] == "OmronFins"
            && protocol["transport"] == "tcp_udp"
            && protocol["telemetryRead"] == true
    }));
    let discoverable = protocols
        .iter()
        .filter_map(|protocol| {
            protocol["automaticDiscovery"]
                .as_bool()
                .filter(|enabled| *enabled)
                .map(|_| protocol["protocolType"].as_str().unwrap())
        })
        .collect::<Vec<_>>();
    assert_eq!(discoverable, vec!["ModbusRtu", "OpcUa"]);
}

#[tokio::test]
async fn dlt645_data_identifier_catalog_is_available_to_console_and_runtime_authors() {
    let (status, payload) = get_json(
        app(AppState::default()),
        "/api/protocols/dlt645/data-identifiers",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let templates = payload.as_array().unwrap();
    assert!(templates.len() >= 16);
    assert!(templates.iter().any(|template| {
        template["templateId"] == "voltage_a"
            && template["dataIdentifier"] == "02010100"
            && template["decimalPlaces"] == 1
            && template["valueBytes"] == 2
            && template["unit"] == "V"
    }));
}

#[tokio::test]
async fn bacnet_ip_catalog_exposes_structured_objects_and_properties() {
    let (status, payload) =
        get_json(app(AppState::default()), "/api/protocols/bacnet-ip/catalog").await;

    assert_eq!(status, StatusCode::OK);
    let object_types = payload["objectTypes"].as_array().unwrap();
    let properties = payload["properties"].as_array().unwrap();
    assert!(object_types.iter().any(|object| {
        object["objectType"] == "analog_input"
            && object["rawValue"] == 0
            && object["writable"] == false
    }));
    assert!(object_types.iter().any(|object| {
        object["objectType"] == "analog_output"
            && object["rawValue"] == 1
            && object["writable"] == true
    }));
    assert!(properties
        .iter()
        .any(|property| { property["property"] == "present_value" && property["rawValue"] == 85 }));
}

#[tokio::test]
async fn empty_bootstrap_keeps_a_new_and_reopened_database_empty() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", tempdir.path().join("empty.db").display());

    let state = AppState::with_sqlite_bootstrap(&database_url, BootstrapMode::Empty)
        .await
        .unwrap();
    {
        let store = state.store.lock().unwrap();
        assert_eq!(store.edge_nodes().count(), 0);
        assert_eq!(store.projects().count(), 0);
        assert_eq!(store.point_sets().count(), 0);
        assert_eq!(store.products().count(), 0);
        assert_eq!(store.releases().count(), 0);
    }
    drop(state);

    let reopened = AppState::with_sqlite_bootstrap(&database_url, BootstrapMode::Empty)
        .await
        .unwrap();
    let store = reopened.store.lock().unwrap();
    assert_eq!(store.edge_nodes().count(), 0);
    assert_eq!(store.projects().count(), 0);
    assert_eq!(store.point_sets().count(), 0);
    assert_eq!(store.products().count(), 0);
    assert_eq!(store.releases().count(), 0);
}

#[tokio::test]
async fn demo_bootstrap_remains_available_for_disposable_environments() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", tempdir.path().join("demo.db").display());

    let state = AppState::with_sqlite_bootstrap(&database_url, BootstrapMode::Demo)
        .await
        .unwrap();
    let store = state.store.lock().unwrap();
    assert!(store.edge_nodes().any(|edge| edge.edge_id == "edge-dev"));
    assert!(store
        .projects()
        .any(|project| project.project_id == "demo-plant"));
    assert!(store.product(SIEMENS_S7_TEMPLATE_ID).is_some());
    assert!(store.product(OMRON_FINS_TEMPLATE_ID).is_some());
}

#[tokio::test]
async fn demo_bootstrap_upgrades_an_existing_catalog_with_manufacturer_templates() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", tempdir.path().join("legacy.db").display());
    let sqlite = SqliteCloudStore::connect(&database_url).await.unwrap();
    sqlite
        .upsert_project(Project::new("demo-plant", "Existing factory"))
        .await
        .unwrap();
    sqlite
        .upsert_edge_node(EdgeNode::new("legacy-edge", "Existing edge"))
        .await
        .unwrap();
    drop(sqlite);

    let state = AppState::with_sqlite_bootstrap(&database_url, BootstrapMode::Demo)
        .await
        .unwrap();
    {
        let store = state.store.lock().unwrap();
        for product_id in [SIEMENS_S7_TEMPLATE_ID, OMRON_FINS_TEMPLATE_ID] {
            let product = store.product(product_id).expect("manufacturer product");
            let version = store
                .product_version(
                    product_id,
                    product.latest_version.as_deref().expect("latest version"),
                )
                .expect("manufacturer product version");
            assert_eq!(version.status, ProductVersionStatus::Published);
            assert_eq!(version.command_flows.len(), 1);
            assert_eq!(version.data_configs.len(), 1);
        }
    }
    drop(state);

    let reopened = AppState::with_sqlite_bootstrap(&database_url, BootstrapMode::Empty)
        .await
        .unwrap();
    let store = reopened.store.lock().unwrap();
    assert!(store.product(SIEMENS_S7_TEMPLATE_ID).is_some());
    assert!(store.product(OMRON_FINS_TEMPLATE_ID).is_some());
}

#[tokio::test]
async fn catalog_api_persists_project_point_set_product_and_version_across_reopen() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", tempdir.path().join("catalog.db").display());
    let router = app(AppState::with_sqlite(&database_url).await.unwrap());

    let (status, _) = send_json(
        router.clone(),
        "POST",
        "/api/projects",
        json!({
            "projectId": "factory-a",
            "name": "一号工厂",
            "environment": "production",
            "owner": "edge-team",
            "description": "生产边缘采集"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        json!({
            "pointSetId": "factory-a-pump-points",
            "projectId": "factory-a",
            "name": "泵站标准点位",
            "protocol": "ModbusRtu",
            "points": [{
                "pointId": "pressure",
                "semanticId": "pump.pressure",
                "address": {
                    "kind": "holding_register",
                    "value": "40011",
                    "modbus": {
                        "encoding": "f32",
                        "byteOrder": "little_endian",
                        "wordOrder": "low_word_first",
                        "scale": 0.1,
                        "offset": -2.0
                    }
                },
                "valueType": "Float",
                "unit": "MPa",
                "intervalMs": 1000
            }]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = send_json(
        router.clone(),
        "POST",
        "/api/products",
        json!({
            "productId": "pump-edge-product",
            "projectId": "factory-a",
            "name": "泵站边缘产品",
            "productType": "pump-station",
            "description": "泵站采集与上报产品"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, version) = send_json(
        router.clone(),
        "POST",
        "/api/products/pump-edge-product/versions",
        json!({
            "version": "v1.0.0",
            "pointSetIds": ["factory-a-pump-points"],
            "deviceModels": [],
            "devices": [],
            "protocolConnections": [],
            "collectionTasks": [],
            "algorithms": [],
            "dataConfigs": [],
            "mqttUplinks": []
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(version["status"], "draft");

    let (status, conflict) = send_json(
        router,
        "DELETE",
        "/api/point-sets/factory-a-pump-points",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        conflict["message"],
        "point set is referenced by a product version"
    );

    let reopened = app(AppState::with_sqlite(&database_url).await.unwrap());
    let (_, projects) = get_json(reopened.clone(), "/api/projects").await;
    let (_, point_sets) = get_json(reopened.clone(), "/api/point-sets").await;
    let (_, products) = get_json(reopened.clone(), "/api/products").await;
    let (_, versions) = get_json(reopened, "/api/products/pump-edge-product/versions").await;

    assert!(projects
        .as_array()
        .unwrap()
        .iter()
        .any(|project| project["projectId"] == "factory-a"));
    let point_set = point_sets
        .as_array()
        .unwrap()
        .iter()
        .find(|point_set| point_set["pointSetId"] == "factory-a-pump-points")
        .unwrap();
    let product = products
        .as_array()
        .unwrap()
        .iter()
        .find(|product| product["productId"] == "pump-edge-product")
        .unwrap();
    assert_eq!(point_set["points"][0]["pointId"], "pressure");
    assert_eq!(
        point_set["points"][0]["address"]["modbus"]["wordOrder"],
        "low_word_first"
    );
    assert_eq!(point_set["points"][0]["address"]["modbus"]["scale"], 0.1);
    assert_eq!(product["productId"], "pump-edge-product");
    assert_eq!(versions[0]["pointSetIds"][0], "factory-a-pump-points");
}

#[tokio::test]
async fn catalog_deletion_enforces_dependencies_and_persists_the_result() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!(
        "sqlite://{}",
        tempdir.path().join("delete-catalog.db").display()
    );
    let router = app(AppState::with_sqlite(&database_url).await.unwrap());

    let (status, published_conflict) = send_json(
        router.clone(),
        "DELETE",
        "/api/products/pump-collection-uplink",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        published_conflict["message"],
        "product bound to an edge cannot be deleted"
    );

    let (status, _) = send_json(
        router.clone(),
        "POST",
        "/api/projects",
        json!({
            "projectId": "delete-project",
            "name": "待删除项目",
            "environment": "test",
            "owner": "catalog-test",
            "description": "验证资源依赖删除顺序"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        json!({
            "pointSetId": "delete-points",
            "projectId": "delete-project",
            "name": "待删除点位集",
            "protocol": "ModbusTcp",
            "points": []
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, project_conflict) = send_json(
        router.clone(),
        "DELETE",
        "/api/projects/delete-project",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        project_conflict["message"],
        "project still owns point sets or products"
    );

    let (status, _) = send_json(
        router.clone(),
        "DELETE",
        "/api/point-sets/delete-points",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = send_json(
        router.clone(),
        "POST",
        "/api/products",
        json!({
            "productId": "delete-product",
            "projectId": "delete-project",
            "name": "待删除草稿产品",
            "productType": "test-device",
            "description": "草稿版本应随产品删除"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = send_json(
        router.clone(),
        "POST",
        "/api/products/delete-product/versions",
        json!({
            "version": "v1.0.0",
            "pointSetIds": [],
            "deviceModels": [],
            "devices": [],
            "protocolConnections": [],
            "collectionTasks": [],
            "algorithms": [],
            "dataConfigs": [],
            "mqttUplinks": []
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = send_json(
        router.clone(),
        "DELETE",
        "/api/products/delete-product",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = send_json(
        router,
        "DELETE",
        "/api/projects/delete-project",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let reopened = app(AppState::with_sqlite(&database_url).await.unwrap());
    let (_, projects) = get_json(reopened.clone(), "/api/projects").await;
    let (_, point_sets) = get_json(reopened.clone(), "/api/point-sets").await;
    let (_, products) = get_json(reopened, "/api/products").await;
    assert!(projects
        .as_array()
        .unwrap()
        .iter()
        .all(|project| project["projectId"] != "delete-project"));
    assert!(point_sets
        .as_array()
        .unwrap()
        .iter()
        .all(|point_set| point_set["pointSetId"] != "delete-points"));
    assert!(products
        .as_array()
        .unwrap()
        .iter()
        .all(|product| product["productId"] != "delete-product"));
}

#[tokio::test]
async fn custom_serial_point_set_requires_a_valid_frame_dsl() {
    let state = AppState::default();
    state
        .store
        .lock()
        .unwrap()
        .upsert_project(cloud_control::Project::new(
            "serial-project",
            "Serial Project",
        ));
    let router = app(state);

    let (invalid_status, invalid) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        json!({
            "pointSetId": "vendor-points",
            "projectId": "serial-project",
            "name": "Vendor points",
            "protocol": "CustomSerial",
            "points": [{
                "pointId": "temperature",
                "semanticId": "sensor.temperature",
                "address": {"kind": "custom_serial_frame", "value": "{not-json}"},
                "valueType": "Float",
                "intervalMs": 1000
            }]
        }),
    )
    .await;
    assert_eq!(invalid_status, StatusCode::BAD_REQUEST);
    assert!(invalid["message"]
        .as_str()
        .unwrap()
        .contains("invalid JSON"));

    let (invalid_version_status, invalid_version) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        json!({
            "pointSetId": "vendor-points",
            "projectId": "serial-project",
            "name": "Vendor points",
            "protocol": "CustomSerial",
            "points": [{
                "pointId": "temperature",
                "semanticId": "sensor.temperature",
                "address": {"kind": "custom_serial_frame", "value": json!({
                    "schemaVersion": 1,
                    "frameEncoding": "slip",
                    "requestHex": "10 02",
                    "valueOffset": 1,
                    "valueEncoding": "u16_be"
                }).to_string()},
                "valueType": "Float",
                "intervalMs": 1000
            }]
        }),
    )
    .await;
    assert_eq!(invalid_version_status, StatusCode::BAD_REQUEST);
    assert!(invalid_version["message"]
        .as_str()
        .unwrap()
        .contains("schemaVersion 1 only supports raw"));

    let frame = json!({
        "schemaVersion": 2,
        "frameEncoding": "cobs",
        "requestHex": "10 02",
        "requestChecksum": "crc16_ccitt_false",
        "responseChecksum": "sum8",
        "responsePrefixHex": "AA",
        "valueOffset": 1,
        "valueEncoding": "u16_be",
        "scale": 0.1,
        "offset": 0.0
    });
    let (valid_status, saved) = send_json(
        router,
        "POST",
        "/api/point-sets",
        json!({
            "pointSetId": "vendor-points",
            "projectId": "serial-project",
            "name": "Vendor points",
            "protocol": "CustomSerial",
            "points": [{
                "pointId": "temperature",
                "semanticId": "sensor.temperature",
                "address": {"kind": "custom_serial_frame", "value": frame.to_string()},
                "valueType": "Float",
                "intervalMs": 1000
            }]
        }),
    )
    .await;

    assert_eq!(valid_status, StatusCode::CREATED);
    assert_eq!(saved["points"][0]["address"]["kind"], "custom_serial_frame");
    let saved_frame: Value =
        serde_json::from_str(saved["points"][0]["address"]["value"].as_str().unwrap()).unwrap();
    assert_eq!(saved_frame["schemaVersion"], 2);
    assert_eq!(saved_frame["frameEncoding"], "cobs");
    assert_eq!(saved_frame["requestChecksum"], "crc16_ccitt_false");
}

#[tokio::test]
async fn dlt645_point_set_requires_a_structured_meter_and_data_identifier_address() {
    let router = app(AppState::default());
    let point_set = |address: Value| {
        json!({
            "pointSetId": "electric-meter-points",
            "projectId": "demo-plant",
            "name": "Electric meter points",
            "protocol": "Dlt645",
            "points": [{
                "pointId": "voltage_a",
                "semanticId": "electric.voltage.a",
                "address": address,
                "valueType": "Float",
                "unit": "V",
                "access": "read_only",
                "intervalMs": 1000
            }]
        })
    };

    let (status, payload) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        point_set(json!({"kind": "holding_register", "value": "40001"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(payload["message"]
        .as_str()
        .unwrap()
        .contains("must use dlt645_address"));

    let (status, payload) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        point_set(json!({"kind": "dlt645_address", "value": "123:02010100:1"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(payload["message"]
        .as_str()
        .unwrap()
        .contains("12 decimal digits"));

    let (status, payload) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        point_set(json!({
            "kind": "dlt645_address",
            "value": "123456789012:F0010203:2"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(payload["message"]
        .as_str()
        .unwrap()
        .contains("requires response value byte length"));

    let (status, payload) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        point_set(json!({
            "kind": "dlt645_address",
            "value": "123456789012:02010100:1:3"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(payload["message"]
        .as_str()
        .unwrap()
        .contains("response length is 2 bytes"));

    let (status, saved) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        point_set(json!({
            "kind": "dlt645_address",
            "value": "123456789012:02010100:1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(saved["points"][0]["address"]["kind"], "dlt645_address");
    assert_eq!(
        saved["points"][0]["address"]["value"],
        "123456789012:02010100:1"
    );

    let mut vendor = point_set(json!({
        "kind": "dlt645_address",
        "value": "123456789012:F0010203:2:4"
    }));
    vendor["pointSetId"] = json!("vendor-electric-meter-points");
    vendor["points"][0]["pointId"] = json!("vendor_energy");
    vendor["points"][0]["semanticId"] = json!("vendor.energy");
    let (status, saved) = send_json(router, "POST", "/api/point-sets", vendor).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        saved["points"][0]["address"]["value"],
        "123456789012:F0010203:2:4"
    );
}

#[tokio::test]
async fn s7_and_fins_point_sets_preserve_runtime_canonical_addresses() {
    let router = app(AppState::default());
    let (status, s7) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        json!({
            "pointSetId": "s7-drive-points",
            "projectId": "demo-plant",
            "name": "S7 drive points",
            "protocol": "SiemensS7",
            "points": [{
                "pointId": "speed",
                "semanticId": "drive.speed",
                "address": { "kind": "s7_address", "value": "DB3.DINT6" },
                "valueType": "Integer",
                "access": "read_write",
                "intervalMs": 500
            }]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{s7}");
    assert_eq!(s7["points"][0]["address"]["kind"], "s7_address");
    assert_eq!(s7["points"][0]["address"]["value"], "DB3.DINT6");

    let (status, fins) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        json!({
            "pointSetId": "fins-machine-points",
            "projectId": "demo-plant",
            "name": "FINS machine points",
            "protocol": "OmronFins",
            "points": [{
                "pointId": "running",
                "semanticId": "machine.running",
                "address": { "kind": "fins_address", "value": "H7.3" },
                "valueType": "Boolean",
                "access": "read_write",
                "intervalMs": 1000
            }]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{fins}");
    assert_eq!(fins["points"][0]["address"]["kind"], "fins_address");
    assert_eq!(fins["points"][0]["address"]["value"], "H7.3");

    let (status, point_sets) = get_json(router, "/api/point-sets").await;
    assert_eq!(status, StatusCode::OK);
    assert!(point_sets.as_array().unwrap().iter().any(|point_set| {
        point_set["pointSetId"] == "s7-drive-points"
            && point_set["points"][0]["address"]["value"] == "DB3.DINT6"
    }));
    assert!(point_sets.as_array().unwrap().iter().any(|point_set| {
        point_set["pointSetId"] == "fins-machine-points"
            && point_set["points"][0]["address"]["value"] == "H7.3"
    }));
}

#[tokio::test]
async fn iec104_point_set_requires_common_address_and_ioa() {
    let router = app(AppState::default());
    let point_set = |address: Value| {
        json!({
            "pointSetId": "iec104-station-points",
            "projectId": "demo-plant",
            "name": "IEC 104 station points",
            "protocol": "Iec104",
            "points": [{
                "pointId": "line_voltage",
                "semanticId": "station.line_voltage",
                "address": address,
                "valueType": "Float",
                "intervalMs": 1000
            }]
        })
    };

    let (status, payload) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        point_set(json!({"kind": "iec101_ioa", "value": "1001"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(payload["message"]
        .as_str()
        .unwrap()
        .contains("must use iec104_ioa"));

    let (status, payload) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        point_set(json!({"kind": "iec104_ioa", "value": "0:1001"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(payload["message"]
        .as_str()
        .unwrap()
        .contains("common address"));

    let (status, saved) = send_json(
        router,
        "POST",
        "/api/point-sets",
        point_set(json!({"kind": "iec104_ioa", "value": "1:1001"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(saved["protocol"], "Iec104");
    assert_eq!(saved["points"][0]["address"]["kind"], "iec104_ioa");
    assert_eq!(saved["points"][0]["address"]["value"], "1:1001");
}

#[tokio::test]
async fn writable_iec104_point_set_requires_a_compatible_control_type() {
    let router = app(AppState::default());
    let request = |iec104: Value, value_type: &str| {
        json!({
            "pointSetId": "iec104-command-points",
            "projectId": "demo-plant",
            "name": "IEC 104 command points",
            "protocol": "Iec104",
            "points": [{
                "pointId": "breaker_close",
                "semanticId": "breaker.close",
                "address": { "kind": "iec104_ioa", "value": "7:1201" },
                "valueType": value_type,
                "access": "read_write",
                "iec104": iec104,
                "intervalMs": 1000
            }]
        })
    };

    let (status, payload) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        request(Value::Null, "Boolean"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(payload["message"].as_str().unwrap().contains("controlType"));

    let (status, payload) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        request(
            json!({
                "controlType": "C_SE_NC_1",
                "selectBeforeOperate": false
            }),
            "Boolean",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(payload["message"]
        .as_str()
        .unwrap()
        .contains("incompatible"));

    let (status, saved) = send_json(
        router,
        "POST",
        "/api/point-sets",
        request(
            json!({
                "controlType": "C_SC_NA_1",
                "selectBeforeOperate": true
            }),
            "Boolean",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(saved["points"][0]["iec104"]["controlType"], "C_SC_NA_1");
    assert_eq!(saved["points"][0]["iec104"]["selectBeforeOperate"], true);
}

#[tokio::test]
async fn writable_iec101_point_set_requires_a_compatible_control_type() {
    let router = app(AppState::default());
    let request = |iec101: Value, value_type: &str| {
        json!({
            "pointSetId": "iec101-command-points",
            "projectId": "demo-plant",
            "name": "IEC 101 command points",
            "protocol": "Iec101",
            "points": [{
                "pointId": "breaker_close",
                "semanticId": "breaker.close",
                "address": { "kind": "iec101_ioa", "value": "1:7:1201" },
                "valueType": value_type,
                "access": "read_write",
                "iec101": iec101,
                "intervalMs": 1000
            }]
        })
    };

    let (status, payload) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        request(Value::Null, "Boolean"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(payload["message"].as_str().unwrap().contains("controlType"));

    let (status, payload) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        request(
            json!({
                "controlType": "C_SE_NC_1",
                "selectBeforeOperate": false
            }),
            "Boolean",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(payload["message"]
        .as_str()
        .unwrap()
        .contains("incompatible"));

    let (status, saved) = send_json(
        router,
        "POST",
        "/api/point-sets",
        request(
            json!({
                "controlType": "C_SC_NA_1",
                "selectBeforeOperate": true
            }),
            "Boolean",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(saved["points"][0]["iec101"]["controlType"], "C_SC_NA_1");
    assert_eq!(saved["points"][0]["iec101"]["selectBeforeOperate"], true);
}

#[tokio::test]
async fn opc_ua_point_set_accepts_validated_semantic_browse_paths() {
    let router = app(AppState::default());
    let point_set = |value: String| {
        json!({
            "pointSetId": "opcua-semantic-points",
            "projectId": "demo-plant",
            "name": "OPC UA semantic points",
            "protocol": "OpcUa",
            "points": [{
                "pointId": "service_level",
                "semanticId": "server.service_level",
                "address": { "kind": "browse_path", "value": value },
                "valueType": "Integer",
                "intervalMs": 1000
            }]
        })
    };

    let (status, payload) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        point_set(json!({ "startingNode": "i=85", "elements": [] }).to_string()),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(payload["message"]
        .as_str()
        .unwrap()
        .contains("requires at least one target element"));

    let browse_path = json!({
        "startingNode": "i=85",
        "elements": [
            { "namespaceIndex": 0, "targetName": "Server" },
            { "namespaceIndex": 0, "targetName": "ServiceLevel" }
        ]
    })
    .to_string();
    let (status, saved) = send_json(
        router,
        "POST",
        "/api/point-sets",
        point_set(browse_path.clone()),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(saved["protocol"], "OpcUa");
    assert_eq!(saved["points"][0]["address"]["kind"], "browse_path");
    assert_eq!(saved["points"][0]["address"]["value"], browse_path);
}

#[tokio::test]
async fn writable_opc_ua_point_set_requires_a_compatible_ua_write_type() {
    let router = app(AppState::default());
    let request = |opc_ua: Value, value_type: &str| {
        json!({
            "pointSetId": "opcua-command-points",
            "projectId": "demo-plant",
            "name": "OPC UA command points",
            "protocol": "OpcUa",
            "points": [{
                "pointId": "speed_setpoint",
                "semanticId": "pump.speed_setpoint",
                "address": { "kind": "node_id", "value": "ns=2;s=Pump/SpeedSetpoint" },
                "valueType": value_type,
                "access": "read_write",
                "opcUa": opc_ua,
                "intervalMs": 1000
            }]
        })
    };

    let (status, payload) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        request(Value::Null, "Integer"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(payload["message"]
        .as_str()
        .unwrap()
        .contains("writeDataType"));

    let (status, payload) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        request(json!({ "writeDataType": "Double" }), "Integer"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(payload["message"]
        .as_str()
        .unwrap()
        .contains("incompatible"));

    let (status, saved) = send_json(
        router,
        "POST",
        "/api/point-sets",
        request(json!({ "writeDataType": "UInt16" }), "Integer"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(saved["points"][0]["opcUa"]["writeDataType"], "UInt16");
}

#[tokio::test]
async fn point_set_rejects_writable_modbus_input_registers() {
    let (status, payload) = send_json(
        app(AppState::default()),
        "POST",
        "/api/point-sets",
        json!({
            "pointSetId": "invalid-writable-inputs",
            "projectId": "demo-plant",
            "name": "Invalid writable inputs",
            "protocol": "ModbusRtu",
            "points": [{
                "pointId": "device_temperature",
                "semanticId": "device.temperature",
                "address": {"kind": "input_register", "value": "30001"},
                "valueType": "Float",
                "access": "read_write",
                "intervalMs": 1000
            }]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(payload["message"]
        .as_str()
        .unwrap()
        .contains("Modbus input_register points are protocol-level read-only"));
}

#[tokio::test]
async fn point_set_rejects_invalid_or_writable_modbus_register_bit_fields() {
    let router = app(AppState::default());
    let point_set = |bit_index: u8, access: &str| {
        json!({
            "pointSetId": format!("bit-field-{bit_index}-{access}"),
            "projectId": "demo-plant",
            "name": "Register bit field",
            "protocol": "ModbusTcp",
            "points": [{
                "pointId": "ready",
                "semanticId": "device.ready",
                "address": {
                    "kind": "holding_register",
                    "value": "40001",
                    "modbus": {
                        "byteOrder": "big_endian",
                        "wordOrder": "high_word_first",
                        "scale": 1.0,
                        "offset": 0.0,
                        "bitIndex": bit_index
                    }
                },
                "valueType": "Boolean",
                "access": access,
                "intervalMs": 1000
            }]
        })
    };

    let (status, payload) = send_json(
        router.clone(),
        "POST",
        "/api/point-sets",
        point_set(16, "read_only"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(payload["message"]
        .as_str()
        .unwrap()
        .contains("between 0 and 15"));

    let (status, payload) = send_json(
        router,
        "POST",
        "/api/point-sets",
        point_set(3, "read_write"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(payload["message"]
        .as_str()
        .unwrap()
        .contains("atomic mask-write"));
}

#[tokio::test]
async fn product_version_rejects_point_sets_from_another_project() {
    let state = AppState::default();
    {
        let mut store = state.store.lock().unwrap();
        store.upsert_project(cloud_control::Project::new("project-a", "A"));
        store.upsert_project(cloud_control::Project::new("project-b", "B"));
        store.upsert_product(cloud_control::Product::new(
            "product-a",
            "project-a",
            "Product A",
            "pump",
        ));
        store.upsert_point_set(cloud_control::PointSet::new(
            "points-b",
            "project-b",
            "Points B",
            edge_core::ProtocolType::ModbusRtu,
        ));
    }

    let (status, payload) = send_json(
        app(state),
        "POST",
        "/api/products/product-a/versions",
        json!({
            "version": "v1.0.0",
            "pointSetIds": ["points-b"]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        payload["message"],
        "product version cannot reference a point set from another project"
    );
}

#[tokio::test]
async fn product_command_flow_only_materializes_writable_points() {
    let state = AppState::default();
    let router = app(state.clone());

    let mut invalid = publishable_version("pump-collection-uplink", "v-command-readonly");
    invalid.command_flows = vec![command_flow_for_point("pump_pressure")];
    let (status, payload) = send_json(
        router.clone(),
        "POST",
        "/api/products/pump-collection-uplink/versions",
        serde_json::to_value(invalid).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        payload["message"],
        "command flow pump-control write node write-point references read-only point pump_pressure"
    );

    let mut valid = publishable_version("pump-collection-uplink", "v-command-write");
    valid.command_flows = vec![command_flow_for_point("pump_running")];
    let (status, created) = send_json(
        router.clone(),
        "POST",
        "/api/products/pump-collection-uplink/versions",
        serde_json::to_value(valid).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["commandFlows"][0]["flow_id"], "pump-control");

    let (status, published) = send_json(
        router.clone(),
        "POST",
        "/api/products/pump-collection-uplink/versions/v-command-write/publish",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(published["status"], "published");

    let (status, edge) = send_json(
        router.clone(),
        "POST",
        "/api/edge-nodes",
        json!({
            "displayName": "指令编排测试边端",
            "projectId": "demo-plant",
            "productId": "pump-collection-uplink",
            "site": "测试/指令"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let edge_id = edge["edgeId"].as_str().unwrap();
    let (status, desired) = get_json(router, &format!("/api/edges/{edge_id}/desired-config")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        desired["package"]["command_flows"][0]["nodes"][1]["ref_id"],
        "pump_running"
    );
    let running = desired["package"]["point_mappings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|point| point["point_id"] == "pump_running")
        .unwrap();
    assert_eq!(running["access"], "read_write");
}

#[tokio::test]
async fn opc_ua_write_type_survives_product_publication_and_edge_materialization() {
    let state = AppState::default();
    {
        let mut store = state.store.lock().unwrap();
        let mut point_set = PointSet::new(
            "opcua-materialized-points",
            "demo-plant",
            "OPC UA materialized points",
            ProtocolType::OpcUa,
        );
        point_set.points.push(PointSetPoint {
            point_id: "speed_setpoint".to_string(),
            semantic_id: "pump.speed_setpoint".to_string(),
            address: PointAddress::opc_ua_node_id("ns=2;s=Pump/SpeedSetpoint"),
            value_type: TelemetryType::Integer,
            access: PointAccess::ReadWrite,
            opc_ua: Some(OpcUaPointOptions::new(OpcUaWriteDataType::UInt16)),
            iec101: None,
            iec104: None,
            bacnet: None,
            unit: Some("rpm".to_string()),
            interval_ms: 500,
        });
        store.upsert_point_set(point_set);

        let mut product = Product::new(
            "opcua-command-product",
            "demo-plant",
            "OPC UA Command Product",
            "pump",
        );
        product.latest_version = Some("v1.0.0".to_string());
        store.upsert_product(product);

        let mut version = ProductVersion::draft("opcua-command-product", "v1.0.0");
        version.status = ProductVersionStatus::Published;
        version.point_set_ids = vec!["opcua-materialized-points".to_string()];
        version.devices = vec![DeviceInstance::new("pump-1", "pump")];
        version.protocol_connections = vec![ProtocolConnection::opc_ua(
            "opcua-main",
            "opc.tcp://127.0.0.1:4840/",
            OpcUaConnectionSettings::default(),
        )];
        version.mqtt_uplinks = vec![MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://127.0.0.1:1883",
            "opcua-materialization-test",
        )];
        version.command_flows = vec![command_flow_for_point("speed_setpoint")];
        store.upsert_product_version(version);
    }

    let router = app(state);
    let (status, edge) = send_json(
        router.clone(),
        "POST",
        "/api/edge-nodes",
        json!({
            "displayName": "OPC UA command edge",
            "projectId": "demo-plant",
            "productId": "opcua-command-product",
            "site": "test/opcua"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let edge_id = edge["edgeId"].as_str().unwrap();

    let (status, desired) = get_json(router, &format!("/api/edges/{edge_id}/desired-config")).await;
    assert_eq!(status, StatusCode::OK);
    let mapping = desired["package"]["point_mappings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|point| point["point_id"] == "speed_setpoint")
        .expect("materialized OPC UA point");
    assert_eq!(mapping["access"], "read_write");
    assert_eq!(mapping["opc_ua"]["writeDataType"], "UInt16");
    assert_eq!(mapping["interval_ms"], 500);
}

#[tokio::test]
async fn iec104_control_options_survive_product_publication_and_edge_materialization() {
    let state = AppState::default();
    {
        let mut store = state.store.lock().unwrap();
        let mut point_set = PointSet::new(
            "iec104-materialized-points",
            "demo-plant",
            "IEC 104 materialized points",
            ProtocolType::Iec104,
        );
        point_set.points.push(PointSetPoint {
            point_id: "breaker_close".to_string(),
            semantic_id: "breaker.close".to_string(),
            address: PointAddress::iec104(7, 1201),
            value_type: TelemetryType::Boolean,
            access: PointAccess::ReadWrite,
            opc_ua: None,
            iec101: None,
            iec104: Some(
                Iec104PointOptions::new(Iec104ControlType::SingleCommand)
                    .with_select_before_operate(true),
            ),
            bacnet: None,
            unit: None,
            interval_ms: 500,
        });
        store.upsert_point_set(point_set);

        let mut product = Product::new(
            "iec104-command-product",
            "demo-plant",
            "IEC 104 Command Product",
            "substation",
        );
        product.latest_version = Some("v1.0.0".to_string());
        store.upsert_product(product);

        let mut version = ProductVersion::draft("iec104-command-product", "v1.0.0");
        version.status = ProductVersionStatus::Published;
        version.point_set_ids = vec!["iec104-materialized-points".to_string()];
        version.devices = vec![DeviceInstance::new("substation-1", "substation")];
        version.protocol_connections =
            vec![
                ProtocolConnection::iec104("iec104-main", "tcp://127.0.0.1:2404")
                    .with_iec104_settings(
                        Iec104ConnectionSettings::default().with_cp56_timezone_offset_minutes(480),
                    ),
            ];
        version.mqtt_uplinks = vec![MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://127.0.0.1:1883",
            "iec104-materialization-test",
        )];
        version.command_flows = vec![command_flow_for_point("breaker_close")];
        store.upsert_product_version(version);
    }

    let router = app(state);
    let (status, edge) = send_json(
        router.clone(),
        "POST",
        "/api/edge-nodes",
        json!({
            "displayName": "IEC 104 command edge",
            "projectId": "demo-plant",
            "productId": "iec104-command-product",
            "site": "test/iec104"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let edge_id = edge["edgeId"].as_str().unwrap();

    let (status, desired) = get_json(router, &format!("/api/edges/{edge_id}/desired-config")).await;
    assert_eq!(status, StatusCode::OK);
    let mapping = desired["package"]["point_mappings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|point| point["point_id"] == "breaker_close")
        .expect("materialized IEC 104 point");
    assert_eq!(mapping["access"], "read_write");
    assert_eq!(mapping["iec104"]["controlType"], "C_SC_NA_1");
    assert_eq!(mapping["iec104"]["selectBeforeOperate"], true);
    assert_eq!(mapping["interval_ms"], 500);
    assert_eq!(
        desired["package"]["protocol_connections"][0]["iec104"]["cp56TimeZoneOffsetMinutes"],
        480
    );
}

#[tokio::test]
async fn iec101_station_timezone_survives_product_edge_materialization() {
    let state = AppState::default();
    {
        let mut store = state.store.lock().unwrap();
        let mut point_set = PointSet::new(
            "iec101-materialized-points",
            "demo-plant",
            "IEC 101 materialized points",
            ProtocolType::Iec101,
        );
        point_set.points.push(PointSetPoint {
            point_id: "breaker_close".to_string(),
            semantic_id: "breaker.close".to_string(),
            address: PointAddress::iec101(1, 7, 1201),
            value_type: TelemetryType::Boolean,
            access: PointAccess::ReadWrite,
            opc_ua: None,
            iec101: Some(
                Iec101PointOptions::new(Iec101ControlType::SingleCommand)
                    .with_select_before_operate(true),
            ),
            iec104: None,
            bacnet: None,
            unit: None,
            interval_ms: 500,
        });
        store.upsert_point_set(point_set);

        let mut product = Product::new(
            "iec101-station-product",
            "demo-plant",
            "IEC 101 Station Product",
            "substation",
        );
        product.latest_version = Some("v1.0.0".to_string());
        store.upsert_product(product);

        let mut version = ProductVersion::draft("iec101-station-product", "v1.0.0");
        version.status = ProductVersionStatus::Published;
        version.point_set_ids = vec!["iec101-materialized-points".to_string()];
        version.devices = vec![DeviceInstance::new("substation-1", "substation")];
        version.protocol_connections = vec![ProtocolConnection::iec101_serial(
            "iec101-main",
            SerialConnectionSettings::new("/dev/ttyUSB1", 9600).with_parity("even"),
        )
        .with_iec101_settings(
            Iec101ConnectionSettings::default().with_cp56_timezone_offset_minutes(480),
        )];
        version.mqtt_uplinks = vec![MqttUplinkConfig::velamq(
            "velamq-main",
            "mqtt://127.0.0.1:1883",
            "iec101-materialization-test",
        )];
        version.command_flows = vec![command_flow_for_point("breaker_close")];
        store.upsert_product_version(version);
    }

    let router = app(state);
    let (status, edge) = send_json(
        router.clone(),
        "POST",
        "/api/edge-nodes",
        json!({
            "displayName": "IEC 101 station edge",
            "projectId": "demo-plant",
            "productId": "iec101-station-product",
            "site": "test/iec101"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let edge_id = edge["edgeId"].as_str().unwrap();

    let (status, desired) = get_json(router, &format!("/api/edges/{edge_id}/desired-config")).await;
    assert_eq!(status, StatusCode::OK);
    let connection = &desired["package"]["protocol_connections"][0];
    assert_eq!(connection["protocol"], "Iec101");
    assert_eq!(connection["serial"]["parity"], "even");
    assert_eq!(connection["iec101"]["cp56TimeZoneOffsetMinutes"], 480);
    let mapping = desired["package"]["point_mappings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|point| point["point_id"] == "breaker_close")
        .expect("materialized IEC 101 point");
    assert_eq!(mapping["access"], "read_write");
    assert_eq!(mapping["iec101"]["controlType"], "C_SC_NA_1");
    assert_eq!(mapping["iec101"]["selectBeforeOperate"], true);
}

#[tokio::test]
async fn product_version_rejects_a_disconnected_visual_graph_before_persisting() {
    let state = AppState::default();
    state.store.lock().unwrap().upsert_product(Product::new(
        "invalid-graph-product",
        "demo-plant",
        "Invalid Graph Product",
        "pump",
    ));
    let mut version = publishable_version("invalid-graph-product", "v1.0.0");
    version.point_set_ids.clear();
    version.data_configs[0].visual_graph.nodes = vec![
        DataConfigGraphNode {
            node_id: "point-pressure".to_string(),
            kind: DataConfigGraphNodeKind::Point,
            label: "pressure".to_string(),
            ref_id: Some("pump_pressure".to_string()),
            params: Default::default(),
            x: 60,
            y: 80,
        },
        DataConfigGraphNode {
            node_id: "mqtt-output".to_string(),
            kind: DataConfigGraphNodeKind::Mqtt,
            label: "MQTT output".to_string(),
            ref_id: Some("factory/{edge_id}/pressure".to_string()),
            params: Default::default(),
            x: 620,
            y: 80,
        },
    ];

    let (status, payload) = send_json(
        app(state.clone()),
        "POST",
        "/api/products/invalid-graph-product/versions",
        serde_json::to_value(version).unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        payload["message"],
        "data config telemetry graph MQTT output mqtt-output is disconnected"
    );
    assert!(state
        .store
        .lock()
        .unwrap()
        .product_versions()
        .all(|candidate| candidate.product_id != "invalid-graph-product"));
}

#[tokio::test]
async fn product_version_round_trips_visual_graph_node_parameters_and_named_ports() {
    let state = AppState::default();
    state.store.lock().unwrap().upsert_product(Product::new(
        "branch-graph-product",
        "demo-plant",
        "Branch Graph Product",
        "pump",
    ));
    let mut version = publishable_version("branch-graph-product", "v1.0.0");
    version.point_set_ids.clear();
    version.data_configs[0].visual_graph.nodes = vec![
        DataConfigGraphNode {
            node_id: "point-pressure".to_string(),
            kind: DataConfigGraphNodeKind::Point,
            label: "pressure".to_string(),
            ref_id: Some("pump_pressure".to_string()),
            params: Default::default(),
            x: 60,
            y: 80,
        },
        DataConfigGraphNode {
            node_id: "mqtt-high".to_string(),
            kind: DataConfigGraphNodeKind::Mqtt,
            label: "high pressure".to_string(),
            ref_id: Some("factory/{edge_id}/pressure/high".to_string()),
            params: [
                ("operator".to_string(), json!("Gte")),
                ("threshold".to_string(), json!(80)),
            ]
            .into_iter()
            .collect(),
            x: 620,
            y: 80,
        },
    ];
    version.data_configs[0].visual_graph.edges = vec![DataConfigGraphEdge {
        edge_id: "pressure-high".to_string(),
        from: "point-pressure".to_string(),
        from_port: Some("matched".to_string()),
        to: "mqtt-high".to_string(),
        to_port: Some("payload".to_string()),
    }];

    let router = app(state);
    let (status, created) = send_json(
        router.clone(),
        "POST",
        "/api/products/branch-graph-product/versions",
        serde_json::to_value(version).unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        created["dataConfigs"][0]["visual_graph"]["nodes"][1]["params"]["threshold"],
        80
    );
    assert_eq!(
        created["dataConfigs"][0]["visual_graph"]["edges"][0]["from_port"],
        "matched"
    );

    let (status, versions) = get_json(router, "/api/products/branch-graph-product/versions").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        versions[0]["dataConfigs"][0]["visual_graph"]["nodes"][1]["params"]["operator"],
        "Gte"
    );
    assert_eq!(
        versions[0]["dataConfigs"][0]["visual_graph"]["edges"][0]["to_port"],
        "payload"
    );
}

#[tokio::test]
async fn product_versions_publish_and_rollback_atomically_across_reopen() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", tempdir.path().join("lifecycle.db").display());
    let state = AppState::with_sqlite(&database_url).await.unwrap();
    let product = Product::new(
        "lifecycle-product",
        "demo-plant",
        "Lifecycle Product",
        "pump",
    );
    state.persist_product(product.clone()).await.unwrap();
    state.store.lock().unwrap().upsert_product(product);
    let edge = EdgeNode::new("lifecycle-edge", "Lifecycle Edge").bind_product(
        "demo-plant",
        "lifecycle-product",
        "bootstrap",
    );
    state.persist_edge_node(edge.clone()).await.unwrap();
    state.store.lock().unwrap().register_edge(edge);

    for version_id in ["v1.0.0", "v1.1.0"] {
        let version = publishable_version("lifecycle-product", version_id);
        state
            .persist_product_version(version.clone())
            .await
            .unwrap();
        state.store.lock().unwrap().upsert_product_version(version);
    }

    let router = app(state);
    let (status, first) = send_json(
        router.clone(),
        "POST",
        "/api/products/lifecycle-product/versions/v1.0.0/publish",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["status"], "published");

    let (status, second) = send_json(
        router.clone(),
        "POST",
        "/api/products/lifecycle-product/versions/v1.1.0/publish",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second["status"], "published");

    let (status, rolled_back) = send_json(
        router.clone(),
        "POST",
        "/api/products/lifecycle-product/versions/v1.0.0/rollback",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rolled_back["status"], "published");

    let (status, _) = send_json(
        router,
        "POST",
        "/api/edges/lifecycle-edge/reported-config",
        json!({
            "desiredVersion": "v1.0.0",
            "reportedVersion": "v1.0.0"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let reopened = app(AppState::with_sqlite(&database_url).await.unwrap());
    let (_, products) = get_json(reopened.clone(), "/api/products").await;
    let (_, versions) = get_json(reopened, "/api/products/lifecycle-product/versions").await;
    let product = products
        .as_array()
        .unwrap()
        .iter()
        .find(|product| product["productId"] == "lifecycle-product")
        .unwrap();
    assert_eq!(product["latestVersion"], "v1.0.0");
    let versions = versions.as_array().unwrap();
    assert_eq!(
        versions
            .iter()
            .find(|version| version["version"] == "v1.0.0")
            .unwrap()["status"],
        "published"
    );
    assert_eq!(
        versions
            .iter()
            .find(|version| version["version"] == "v1.1.0")
            .unwrap()["status"],
        "retired"
    );

    let reopened_state = AppState::with_sqlite(&database_url).await.unwrap();
    let store = reopened_state.store.lock().unwrap();
    let edge = store
        .edge_nodes()
        .find(|edge| edge.edge_id == "lifecycle-edge")
        .unwrap();
    assert_eq!(edge.desired_product_version.as_deref(), Some("v1.0.0"));
    assert_eq!(edge.reported_product_version.as_deref(), Some("v1.0.0"));
    assert!(store.config_package("lifecycle-edge", "v1.0.0").is_some());
    assert!(store.config_package("lifecycle-edge", "v1.1.0").is_some());
    let releases = store
        .releases()
        .filter(|release| release.edge_id == "lifecycle-edge")
        .collect::<Vec<_>>();
    assert_eq!(releases.len(), 3);
    assert_eq!(
        releases
            .iter()
            .filter(|release| release.status == ReleaseStatus::Pending)
            .count(),
        0
    );
    assert!(releases.iter().any(|release| {
        release.status == ReleaseStatus::Applied
            && release.desired_version == "v1.0.0"
            && release.reported_version.as_deref() == Some("v1.0.0")
    }));
    assert_eq!(
        releases
            .iter()
            .filter(|release| release.status == ReleaseStatus::Superseded)
            .count(),
        2
    );
}

#[tokio::test]
async fn product_version_publish_rejects_incomplete_draft() {
    let state = AppState::default();
    {
        let mut store = state.store.lock().unwrap();
        store.upsert_product(Product::new(
            "incomplete-product",
            "demo-plant",
            "Incomplete Product",
            "pump",
        ));
        store.upsert_product_version(ProductVersion::draft("incomplete-product", "v1.0.0"));
    }

    let (status, payload) = send_json(
        app(state),
        "POST",
        "/api/products/incomplete-product/versions/v1.0.0/publish",
        Value::Null,
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        payload["message"],
        "product version requires at least one point set before publishing"
    );
}

#[tokio::test]
async fn edge_product_binding_materializes_config_and_survives_reopen() {
    let tempdir = tempfile::tempdir().unwrap();
    let database_url = format!("sqlite://{}", tempdir.path().join("binding.db").display());
    let router = app(AppState::with_sqlite(&database_url).await.unwrap());

    let (status, edge) = send_json(
        router.clone(),
        "POST",
        "/api/edge-nodes",
        json!({
            "displayName": "泵站二号边端",
            "projectId": "demo-plant",
            "productId": "pump-collection-uplink",
            "site": "泵站/二号"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(edge["projectId"], "demo-plant");
    assert_eq!(edge["productId"], "pump-collection-uplink");
    assert_eq!(edge["desiredProductVersion"], "v1.4.3");
    let access_token = edge["accessToken"].as_str().unwrap().to_string();
    assert!(access_token.starts_with("edge_"));
    let edge_id = edge["edgeId"].as_str().unwrap();

    let (_, desired) = get_json(router, &format!("/api/edges/{edge_id}/desired-config")).await;
    assert_eq!(desired["desiredVersion"], "v1.4.3");
    assert_eq!(
        desired["package"]["point_mappings"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        desired["package"]["mqtt_uplinks"][0]["client_id"],
        format!("{edge_id}-runtime-dev")
    );

    let reopened_state = AppState::with_sqlite(&database_url).await.unwrap();
    {
        let store = reopened_state.store.lock().unwrap();
        let credentials = store
            .edge_credentials()
            .filter(|credential| credential.edge_id == edge_id)
            .collect::<Vec<_>>();
        assert_eq!(credentials.len(), 1);
        assert!(credentials[0].active);
        assert_ne!(credentials[0].token_hash, access_token);
        assert_eq!(credentials[0].token_hash.len(), 64);
    }
    let reopened = app(reopened_state);
    let (_, edges) = get_json(reopened, "/api/edge-nodes").await;
    let restored = edges
        .as_array()
        .unwrap()
        .iter()
        .find(|edge| edge["edgeId"] == edge_id)
        .unwrap();
    assert_eq!(restored["productId"], "pump-collection-uplink");
    assert_eq!(restored["desiredProductVersion"], "v1.4.3");
    assert!(restored.get("accessToken").is_none());
}

#[tokio::test]
async fn manufacturer_product_binding_materializes_complete_runtime_config() {
    let router = app(AppState::default());

    for (product_id, protocol, settings_key, writable_point_id) in [
        (
            SIEMENS_S7_TEMPLATE_ID,
            "SiemensS7",
            "siemens_s7",
            "s7_start_command",
        ),
        (
            OMRON_FINS_TEMPLATE_ID,
            "OmronFins",
            "omron_fins",
            "fins_start_command",
        ),
    ] {
        let (status, edge) = send_json(
            router.clone(),
            "POST",
            "/api/edge-nodes",
            json!({
                "displayName": format!("{product_id} test edge"),
                "projectId": "demo-plant",
                "productId": product_id,
                "site": "factory/lab"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{product_id}: {edge}");
        assert_eq!(edge["productId"], product_id);
        assert_eq!(edge["desiredProductVersion"], "v1.0.0");

        let edge_id = edge["edgeId"].as_str().unwrap();
        let (status, desired) = get_json(
            router.clone(),
            &format!("/api/edges/{edge_id}/desired-config"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{product_id}: {desired}");
        assert_eq!(desired["desiredVersion"], "v1.0.0");

        let package = &desired["package"];
        let connections = package["protocol_connections"].as_array().unwrap();
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0]["protocol"], protocol);
        assert!(
            connections[0][settings_key].is_object(),
            "{product_id} must preserve its protocol-specific settings"
        );

        assert_eq!(package["collection_tasks"].as_array().unwrap().len(), 1);
        assert_eq!(package["data_configs"].as_array().unwrap().len(), 1);
        assert_eq!(package["command_flows"].as_array().unwrap().len(), 1);
        assert_eq!(package["mqtt_uplinks"].as_array().unwrap().len(), 1);
        assert_eq!(
            package["mqtt_uplinks"][0]["client_id"],
            format!("{edge_id}-{product_id}")
        );

        let writable_point = package["point_mappings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|point| point["point_id"] == writable_point_id)
            .expect("manufacturer template must expose its command point");
        assert_eq!(writable_point["access"], "read_write");

        let write_node = package["command_flows"][0]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["kind"] == "point_write")
            .expect("command flow must include a write node");
        assert_eq!(write_node["ref_id"], writable_point_id);
    }
}

#[tokio::test]
async fn multi_protocol_product_routes_each_point_through_its_data_config() {
    let state = AppState::default();
    {
        let mut store = state.store.lock().unwrap();
        let mut modbus_points = PointSet::new(
            "multi-modbus-points",
            "demo-plant",
            "Modbus points",
            ProtocolType::ModbusTcp,
        );
        modbus_points.points.push(PointSetPoint {
            point_id: "multi_pressure".to_string(),
            semantic_id: "pump.pressure".to_string(),
            address: PointAddress::modbus_holding_register(40011),
            value_type: TelemetryType::Integer,
            access: PointAccess::ReadOnly,
            opc_ua: None,
            iec101: None,
            iec104: None,
            bacnet: None,
            unit: Some("kPa".to_string()),
            interval_ms: 1_000,
        });
        store.upsert_point_set(modbus_points);

        let mut s7_points = PointSet::new(
            "multi-s7-points",
            "demo-plant",
            "S7 points",
            ProtocolType::SiemensS7,
        );
        s7_points.points.push(PointSetPoint {
            point_id: "multi_speed".to_string(),
            semantic_id: "drive.speed".to_string(),
            address: PointAddress::siemens_s7("DB1.DINT6"),
            value_type: TelemetryType::Integer,
            access: PointAccess::ReadOnly,
            opc_ua: None,
            iec101: None,
            iec104: None,
            bacnet: None,
            unit: Some("rpm".to_string()),
            interval_ms: 500,
        });
        store.upsert_point_set(s7_points);

        let mut product = Product::new(
            "multi-protocol-product",
            "demo-plant",
            "Multi protocol product",
            "production-line",
        );
        product.latest_version = Some("v1.0.0".to_string());
        store.upsert_product(product);

        let mut modbus_config = DataConfig::new(
            "modbus-flow",
            "Modbus flow",
            "pump-1",
            "modbus-main",
            DataConfigCollection::new(1_000),
            DataConfigPublish::new(
                "mqtt-main",
                "factory/{edge_id}/modbus",
                DataConfigPayload::object(),
            ),
        );
        modbus_config.points.push(DataConfigPoint::new(
            "multi_pressure",
            "pump.pressure",
            PointAddress::modbus_holding_register(40011),
            TelemetryType::Integer,
            "pressure",
        ));
        let mut s7_config = DataConfig::new(
            "s7-flow",
            "S7 flow",
            "drive-1",
            "s7-main",
            DataConfigCollection::new(500),
            DataConfigPublish::new(
                "mqtt-main",
                "factory/{edge_id}/s7",
                DataConfigPayload::object(),
            ),
        );
        s7_config.points.push(DataConfigPoint::new(
            "multi_speed",
            "drive.speed",
            PointAddress::siemens_s7("DB1.DINT6"),
            TelemetryType::Integer,
            "speed",
        ));

        let mut version = ProductVersion::draft("multi-protocol-product", "v1.0.0");
        version.status = ProductVersionStatus::Published;
        version.point_set_ids = vec![
            "multi-modbus-points".to_string(),
            "multi-s7-points".to_string(),
        ];
        version.devices = vec![
            DeviceInstance::new("pump-1", "pump"),
            DeviceInstance::new("drive-1", "drive"),
        ];
        version.protocol_connections = vec![
            ProtocolConnection::modbus_tcp("modbus-main", "tcp://127.0.0.1:1502"),
            ProtocolConnection::siemens_s7("s7-main", "s7://127.0.0.1:11102", Default::default()),
        ];
        version.data_configs = vec![modbus_config, s7_config];
        version.mqtt_uplinks = vec![MqttUplinkConfig::velamq(
            "mqtt-main",
            "mqtt://127.0.0.1:1883",
            "multi-protocol-test",
        )];
        store.upsert_product_version(version);
    }

    let router = app(state);
    let (status, edge) = send_json(
        router.clone(),
        "POST",
        "/api/edge-nodes",
        json!({
            "displayName": "Multi protocol edge",
            "projectId": "demo-plant",
            "productId": "multi-protocol-product",
            "site": "test/multi"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{edge}");
    let edge_id = edge["edgeId"].as_str().unwrap();
    let (_, desired) = get_json(router, &format!("/api/edges/{edge_id}/desired-config")).await;
    let mappings = desired["package"]["point_mappings"].as_array().unwrap();
    let modbus = mappings
        .iter()
        .find(|mapping| mapping["point_id"] == "multi_pressure")
        .unwrap();
    let s7 = mappings
        .iter()
        .find(|mapping| mapping["point_id"] == "multi_speed")
        .unwrap();
    assert_eq!(modbus["device_id"], "pump-1");
    assert_eq!(modbus["protocol_connection_id"], "modbus-main");
    assert_eq!(s7["device_id"], "drive-1");
    assert_eq!(s7["protocol_connection_id"], "s7-main");
}

#[tokio::test]
async fn edge_access_token_regeneration_revokes_the_previous_hash() {
    let state = AppState::default();
    let router = app(state.clone());
    let (status, edge) = send_json(
        router.clone(),
        "POST",
        "/api/edge-nodes",
        json!({
            "displayName": "Token 测试边端",
            "projectId": "demo-plant",
            "productId": "pump-collection-uplink",
            "site": "测试/接入"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let edge_id = edge["edgeId"].as_str().unwrap();
    let first_token = edge["accessToken"].as_str().unwrap();

    let (status, regenerated) = send_json(
        router,
        "POST",
        &format!("/api/edge-nodes/{edge_id}/access-token"),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let second_token = regenerated["accessToken"].as_str().unwrap();
    assert_ne!(first_token, second_token);

    let store = state.store.lock().unwrap();
    let credentials = store
        .edge_credentials()
        .filter(|credential| credential.edge_id == edge_id)
        .collect::<Vec<_>>();
    assert_eq!(credentials.len(), 2);
    assert_eq!(
        credentials
            .iter()
            .filter(|credential| credential.active)
            .count(),
        1
    );
    assert!(credentials.iter().all(|credential| {
        credential.token_hash != first_token
            && credential.token_hash != second_token
            && credential.token_hash.len() == 64
    }));
}

#[tokio::test]
async fn edge_binding_rejects_draft_product_versions() {
    let state = AppState::default();
    let mut draft = publishable_version("pump-collection-uplink", "v-next");
    draft.status = ProductVersionStatus::Draft;
    state.store.lock().unwrap().upsert_product_version(draft);

    let (status, payload) = send_json(
        app(state),
        "PUT",
        "/api/edge-nodes/edge-dev/product-binding",
        json!({
            "projectId": "demo-plant",
            "productId": "pump-collection-uplink",
            "desiredVersion": "v-next"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        payload["message"],
        "only published product versions can be bound to an edge"
    );
}

fn publishable_version(product_id: &str, version_id: &str) -> ProductVersion {
    let mut version = ProductVersion::draft(product_id, version_id);
    version.status = ProductVersionStatus::Draft;
    version.point_set_ids = vec!["pump-standard-points".to_string()];
    version.devices = vec![DeviceInstance::new("device-1", "pump")];
    version.protocol_connections = vec![ProtocolConnection::simulated("simulated-main")];
    version.mqtt_uplinks = vec![MqttUplinkConfig::velamq(
        "velamq-main",
        "mqtt://127.0.0.1:1883",
        "lifecycle-test",
    )];
    version.data_configs = vec![DataConfig::new(
        "telemetry",
        "Telemetry",
        "device-1",
        "simulated-main",
        DataConfigCollection::new(1000),
        DataConfigPublish::new(
            "velamq-main",
            "factory/{edge_id}/{device_id}/telemetry",
            DataConfigPayload::object(),
        ),
    )
    .with_point(DataConfigPoint::new(
        "pump_pressure",
        "pump.pressure",
        PointAddress::modbus_holding_register(40011),
        TelemetryType::Float,
        "pressure",
    ))];
    version
}

fn command_flow_for_point(point_id: &str) -> CommandFlowConfig {
    CommandFlowConfig::new(
        "pump-control",
        "Pump control",
        "velamq-main",
        "factory/{edge_id}/command",
        "factory/{edge_id}/reply/{command_id}",
    )
    .with_node(CommandGraphNode::new(
        "input",
        CommandGraphNodeKind::MqttInput,
        "MQTT input",
    ))
    .with_node(
        CommandGraphNode::new(
            "write-point",
            CommandGraphNodeKind::PointWrite,
            "Write point",
        )
        .with_ref(point_id),
    )
    .with_node(CommandGraphNode::new(
        "reply",
        CommandGraphNodeKind::MqttReply,
        "MQTT reply",
    ))
    .with_edge(CommandGraphEdge::new("input-write", "input", "write-point"))
    .with_edge(CommandGraphEdge::new("write-reply", "write-point", "reply"))
}
