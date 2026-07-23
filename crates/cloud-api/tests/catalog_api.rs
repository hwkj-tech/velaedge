use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use cloud_api::{app, AppState, BootstrapMode};
use cloud_control::{EdgeNode, Product, ProductVersion, ProductVersionStatus, ReleaseStatus};
use edge_core::{
    DataConfig, DataConfigCollection, DataConfigGraphEdge, DataConfigGraphNode,
    DataConfigGraphNodeKind, DataConfigPayload, DataConfigPoint, DataConfigPublish, DeviceInstance,
    MqttUplinkConfig, PointAddress, ProtocolConnection, TelemetryType,
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
                "address": {"kind": "holding_register", "value": "40011"},
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
    assert_eq!(product["productId"], "pump-edge-product");
    assert_eq!(versions[0]["pointSetIds"][0], "factory-a-pump-points");
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

    let frame = json!({
        "requestHex": "10 02",
        "requestChecksum": "sum8",
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
