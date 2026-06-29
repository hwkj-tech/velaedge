# Dynamic Data Configuration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build cloud-configurable edge data configurations where each config defines protocol collection, period, point mappings, JSON payload, and MQTT publishing in one unit.

**Architecture:** Add `DataConfig` to `edge-core::EdgeConfigPackage`, then make runtime execute data configs as canonical flows. Cloud API stores and validates data configs inside edge config packages, and the console exposes a list-first `数据配置` page with a step-based dialog editor.

**Tech Stack:** Rust workspace (`edge-core`, `edge-runtime`, `cloud-control`, `cloud-api`), SQLite-backed cloud store, RocksDB runtime store, React/Vite/Vitest console, MQTT publisher abstraction.

---

## File Structure

- Modify `crates/edge-core/src/config.rs`: add `DataConfig`, point, collection, publish, and payload models.
- Modify `crates/edge-core/src/lib.rs`: export new data config types.
- Modify `crates/edge-core/tests/config_contract.rs`: serialization and config-shape tests.
- Modify `crates/cloud-control/src/validation.rs`: validate data configs with sinks, protocol connections, devices, and point fields.
- Modify `crates/cloud-control/tests/config_loop.rs`: validation tests for bad data configs.
- Modify `crates/edge-runtime/src/mqtt_uplink.rs`: add data-config-based MQTT message builder.
- Modify `crates/edge-runtime/src/configured_runtime.rs`: execute data configs and publish one message per config.
- Modify `crates/edge-runtime/tests/configured_runtime.rs` and `crates/edge-runtime/tests/mqtt_uplink.rs`: runtime publishing tests.
- Modify `crates/cloud-api/src/api.rs`: add data config CRUD endpoints and response/request DTOs.
- Modify `crates/cloud-api/src/state.rs`: seed default data configs.
- Modify `crates/cloud-api/tests/api.rs`: API tests.
- Modify `web/console/src/api/types.ts` and `web/console/src/api/client.ts`: data config API types and client methods.
- Create `web/console/src/pages/DataConfigsPage.tsx`: list and step dialog editor.
- Create `web/console/src/pages/DataConfigsPage.test.tsx`: page interaction tests.
- Modify `web/console/src/layout/AppShell.tsx`, `web/console/src/App.tsx`, and `web/console/src/App.test.tsx`: navigation and integration.

---

### Task 1: Core Data Config Contract

**Files:**
- Modify: `crates/edge-core/src/config.rs`
- Modify: `crates/edge-core/src/lib.rs`
- Test: `crates/edge-core/tests/config_contract.rs`

- [ ] **Step 1: Write failing serialization test**

Add this test to `crates/edge-core/tests/config_contract.rs`:

```rust
#[test]
fn config_package_contains_data_configs_for_grouped_mqtt_publishing() {
    use edge_core::{
        DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint,
        DataConfigPublish, EdgeConfigPackage, MqttUplinkConfig, PointAddress,
        TelemetryType,
    };

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
    assert_eq!(json["data_configs"][0]["points"][0]["json_field"], "pressure");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p edge-core config_package_contains_data_configs_for_grouped_mqtt_publishing
```

Expected: FAIL because `DataConfig` types and `with_data_config` do not exist.

- [ ] **Step 3: Add core models**

In `crates/edge-core/src/config.rs`, add `data_configs` to `EdgeConfigPackage` and initialize it:

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EdgeConfigPackage {
    pub edge_id: String,
    pub version: String,
    pub device_models: Vec<DeviceSpec>,
    pub devices: Vec<DeviceInstance>,
    pub protocol_connections: Vec<ProtocolConnection>,
    #[serde(default)]
    pub mqtt_uplinks: Vec<MqttUplinkConfig>,
    #[serde(default)]
    pub data_configs: Vec<DataConfig>,
    pub point_mappings: Vec<TelemetryPointMapping>,
    pub collection_tasks: Vec<CollectionTask>,
    pub algorithms: Vec<AlgorithmSpec>,
}
```

Update `EdgeConfigPackage::new`:

```rust
data_configs: Vec::new(),
```

Add builder:

```rust
pub fn with_data_config(mut self, data_config: DataConfig) -> Self {
    self.data_configs.push(data_config);
    self
}
```

Add these structs below `MqttUplinkConfig`:

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DataConfig {
    pub config_id: String,
    pub name: String,
    pub enabled: bool,
    pub device_id: String,
    pub protocol_connection_id: String,
    pub collection: DataConfigCollection,
    pub points: Vec<DataConfigPoint>,
    pub publish: DataConfigPublish,
}

impl DataConfig {
    pub fn new(
        config_id: impl Into<String>,
        name: impl Into<String>,
        device_id: impl Into<String>,
        protocol_connection_id: impl Into<String>,
        collection: DataConfigCollection,
        publish: DataConfigPublish,
    ) -> Self {
        Self {
            config_id: config_id.into(),
            name: name.into(),
            enabled: true,
            device_id: device_id.into(),
            protocol_connection_id: protocol_connection_id.into(),
            collection,
            points: Vec::new(),
            publish,
        }
    }

    pub fn with_point(mut self, point: DataConfigPoint) -> Self {
        self.points.push(point);
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataConfigCollection {
    pub period_ms: u64,
    pub timeout_ms: u64,
    pub retry_count: u32,
}

impl DataConfigCollection {
    pub fn new(period_ms: u64) -> Self {
        Self {
            period_ms,
            timeout_ms: 800,
            retry_count: 2,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DataConfigPoint {
    pub point_id: String,
    pub semantic_id: String,
    pub address: PointAddress,
    pub value_type: TelemetryType,
    pub unit: Option<String>,
    pub json_field: String,
}

impl DataConfigPoint {
    pub fn new(
        point_id: impl Into<String>,
        semantic_id: impl Into<String>,
        address: PointAddress,
        value_type: TelemetryType,
        json_field: impl Into<String>,
    ) -> Self {
        Self {
            point_id: point_id.into(),
            semantic_id: semantic_id.into(),
            address,
            value_type,
            unit: None,
            json_field: json_field.into(),
        }
    }

    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataConfigPublish {
    pub sink_id: String,
    pub topic_template: String,
    pub qos: u8,
    pub payload: DataConfigPayload,
}

impl DataConfigPublish {
    pub fn new(
        sink_id: impl Into<String>,
        topic_template: impl Into<String>,
        payload: DataConfigPayload,
    ) -> Self {
        Self {
            sink_id: sink_id.into(),
            topic_template: topic_template.into(),
            qos: 1,
            payload,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataConfigPayload {
    pub mode: DataConfigPayloadMode,
    pub timestamp_field: String,
    pub include_quality: bool,
}

impl DataConfigPayload {
    pub fn object() -> Self {
        Self {
            mode: DataConfigPayloadMode::Object,
            timestamp_field: "ts".to_string(),
            include_quality: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DataConfigPayloadMode {
    Object,
    Array,
}
```

- [ ] **Step 4: Export models**

In `crates/edge-core/src/lib.rs`, add exports:

```rust
DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPayloadMode,
DataConfigPoint, DataConfigPublish,
```

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test -p edge-core config_package_contains_data_configs_for_grouped_mqtt_publishing
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/edge-core/src/config.rs crates/edge-core/src/lib.rs crates/edge-core/tests/config_contract.rs
git commit -m "feat: add data config core model"
```

---

### Task 2: Runtime MQTT Payload Builder

**Files:**
- Modify: `crates/edge-runtime/src/mqtt_uplink.rs`
- Test: `crates/edge-runtime/tests/mqtt_uplink.rs`

- [ ] **Step 1: Write failing runtime message builder test**

Add to `crates/edge-runtime/tests/mqtt_uplink.rs`:

```rust
#[test]
fn data_config_builds_one_json_message_per_config() {
    use edge_core::{
        DataConfig, DataConfigCollection, DataConfigPayload, DataConfigPoint,
        DataConfigPublish, DeviceInstance, EdgeConfigPackage, MqttUplinkConfig,
        PointAddress, TelemetrySample, TelemetryType, TelemetryValue,
    };
    use edge_runtime::build_data_config_mqtt_publish_messages;

    let package = EdgeConfigPackage::new("edge-dev", "v1")
        .with_device(DeviceInstance::new("pump-1", "pump"))
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
                    "factory/{edge_id}/{device_id}/status",
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

    let samples = vec![
        TelemetrySample::new(
            "pump-1",
            "pressure",
            TelemetryValue::Float(0.82),
            DataQuality::Good,
            chrono::Utc::now(),
        ),
        TelemetrySample::new(
            "pump-1",
            "running",
            TelemetryValue::Boolean(true),
            DataQuality::Good,
            chrono::Utc::now(),
        ),
    ];

    let messages = build_data_config_mqtt_publish_messages(&package, &samples).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].topic, "factory/edge-dev/pump-1/status");

    let payload: serde_json::Value = serde_json::from_slice(&messages[0].payload).unwrap();
    assert_eq!(payload["edge_id"], "edge-dev");
    assert_eq!(payload["device_id"], "pump-1");
    assert_eq!(payload["values"]["pressure"], 0.82);
    assert_eq!(payload["values"]["running"], true);
    assert_eq!(payload["quality"]["pressure"], "good");
}
```

- [ ] **Step 2: Run failing test**

```bash
cargo test -p edge-runtime data_config_builds_one_json_message_per_config
```

Expected: FAIL because `build_data_config_mqtt_publish_messages` does not exist.

- [ ] **Step 3: Implement builder**

In `crates/edge-runtime/src/mqtt_uplink.rs`, add:

```rust
pub fn build_data_config_mqtt_publish_messages(
    package: &EdgeConfigPackage,
    samples: &[TelemetrySample],
) -> Result<Vec<MqttPublishMessage>> {
    let mut messages = Vec::new();
    for data_config in &package.data_configs {
        if !data_config.enabled {
            continue;
        }
        let uplink = package
            .mqtt_uplinks
            .iter()
            .find(|uplink| uplink.sink_id == data_config.publish.sink_id)
            .ok_or_else(|| anyhow::anyhow!("mqtt sink not found: {}", data_config.publish.sink_id))?;
        validate_uplink(uplink)?;

        let selected = data_config
            .points
            .iter()
            .filter_map(|point| {
                samples
                    .iter()
                    .find(|sample| {
                        sample.device_id == data_config.device_id
                            && sample.telemetry_id == point.point_id
                    })
                    .map(|sample| (point, sample))
            })
            .collect::<Vec<_>>();

        if selected.is_empty() {
            continue;
        }

        let payload = build_data_config_payload(package, data_config, &selected)?;
        messages.push(MqttPublishMessage {
            sink_id: uplink.sink_id.clone(),
            broker: uplink.broker.clone(),
            client_id: uplink.client_id.clone(),
            topic: render_data_config_topic(package, data_config),
            qos: data_config.publish.qos,
            payload,
        });
    }
    Ok(messages)
}
```

Add helper functions in the same file:

```rust
fn build_data_config_payload(
    package: &EdgeConfigPackage,
    data_config: &edge_core::DataConfig,
    selected: &[(&edge_core::DataConfigPoint, &TelemetrySample)],
) -> Result<Vec<u8>> {
    let mut values = serde_json::Map::new();
    let mut quality = serde_json::Map::new();
    let timestamp = selected
        .first()
        .map(|(_, sample)| sample.timestamp)
        .unwrap_or_default();

    for (point, sample) in selected {
        values.insert(point.json_field.clone(), serde_json::to_value(&sample.value)?);
        quality.insert(
            point.json_field.clone(),
            serde_json::to_value(format!("{:?}", sample.quality).to_ascii_lowercase())?,
        );
    }

    let payload = serde_json::json!({
        "edge_id": package.edge_id,
        "device_id": data_config.device_id,
        data_config.publish.payload.timestamp_field.clone(): timestamp,
        "values": values,
        "quality": quality,
    });
    Ok(serde_json::to_vec(&payload)?)
}

fn render_data_config_topic(
    package: &EdgeConfigPackage,
    data_config: &edge_core::DataConfig,
) -> String {
    data_config
        .publish
        .topic_template
        .replace("{edge_id}", &package.edge_id)
        .replace("{device_id}", &data_config.device_id)
        .replace("{config_id}", &data_config.config_id)
        .replace("{site}", "default")
}
```

- [ ] **Step 4: Export builder**

In `crates/edge-runtime/src/lib.rs`, export:

```rust
build_data_config_mqtt_publish_messages,
```

- [ ] **Step 5: Run test**

```bash
cargo test -p edge-runtime data_config_builds_one_json_message_per_config
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/edge-runtime/src/mqtt_uplink.rs crates/edge-runtime/src/lib.rs crates/edge-runtime/tests/mqtt_uplink.rs
git commit -m "feat: build mqtt messages from data configs"
```

---

### Task 3: Runtime Data Config Execution

**Files:**
- Modify: `crates/edge-runtime/src/configured_runtime.rs`
- Test: `crates/edge-runtime/tests/configured_runtime.rs`

- [ ] **Step 1: Write failing integration test**

Add a test proving two data configs publish two MQTT messages:

```rust
#[tokio::test]
async fn configured_runtime_publishes_one_mqtt_message_per_data_config() {
    let package = package_with_two_modbus_data_configs();
    let bus_factory = ScriptedSerialBusFactory::new(vec![
        (
            "meter-rs485-bus-1".to_string(),
            ScriptedSerialBus::new(vec![
                response(1, &[220, 1, 158]),
                response(1, &[1290, 19, 7]),
            ]),
        ),
    ]);
    let mut runtime = ConfiguredEdgeRuntime::new(package, bus_factory).unwrap();
    let mut publisher = RecordingMqttPublisher::default();

    let report = runtime.collect_data_configs_once_and_publish_mqtt(&mut publisher).await.unwrap();

    assert_eq!(report.collection.samples_collected, 6);
    assert_eq!(report.mqtt_messages_published, 2);
    assert_eq!(publisher.messages().len(), 2);
    assert!(publisher.messages()[0].topic.contains("/status"));
    assert!(publisher.messages()[1].topic.contains("/energy"));
}
```

- [ ] **Step 2: Run failing test**

```bash
cargo test -p edge-runtime configured_runtime_publishes_one_mqtt_message_per_data_config
```

Expected: FAIL because runtime method and helper fixtures do not exist.

- [ ] **Step 3: Implement runtime method**

In `ConfiguredEdgeRuntime`, add:

```rust
pub async fn collect_data_configs_once_and_publish_mqtt<P>(
    &mut self,
    publisher: &mut P,
) -> Result<ConfiguredMqttCollectionReport>
where
    P: MqttPublisher + ?Sized,
{
    let mut all_samples = Vec::new();
    for data_config in self.package.data_configs.clone() {
        if !data_config.enabled {
            continue;
        }
        let mappings = data_config
            .points
            .iter()
            .map(|point| {
                TelemetryPointMapping::new(
                    point.point_id.clone(),
                    data_config.device_id.clone(),
                    point.semantic_id.clone(),
                    data_config.protocol_connection_id.clone(),
                    point.address.clone(),
                    point.value_type,
                )
            })
            .collect::<Vec<_>>();
        let mut samples = self.collect_mappings(mappings).await?;
        all_samples.append(&mut samples);
    }
    let mqtt_messages_published =
        publish_data_config_mqtt_samples(&self.package, &all_samples, publisher).await?;
    Ok(ConfiguredMqttCollectionReport {
        collection: CollectionReport {
            samples_collected: all_samples.len(),
        },
        mqtt_messages_published,
    })
}
```

Add `publish_data_config_mqtt_samples` beside `publish_mqtt_samples` or call the builder directly.

- [ ] **Step 4: Run test**

```bash
cargo test -p edge-runtime configured_runtime_publishes_one_mqtt_message_per_data_config
```

Expected: PASS.

- [ ] **Step 5: Run compatibility tests**

```bash
cargo test -p edge-runtime configured_runtime_publishes_modbus_samples_to_mqtt_uplink
```

Expected: PASS, proving the old fallback remains usable.

- [ ] **Step 6: Commit**

```bash
git add crates/edge-runtime/src/configured_runtime.rs crates/edge-runtime/src/mqtt_uplink.rs crates/edge-runtime/tests/configured_runtime.rs
git commit -m "feat: execute data configs in runtime"
```

---

### Task 4: Cloud Validation And API

**Files:**
- Modify: `crates/cloud-control/src/validation.rs`
- Modify: `crates/cloud-api/src/api.rs`
- Modify: `crates/cloud-api/src/state.rs`
- Test: `crates/cloud-control/tests/config_loop.rs`
- Test: `crates/cloud-api/tests/api.rs`

- [ ] **Step 1: Write validation test**

Add a test that rejects a data config with a missing MQTT sink:

```rust
#[test]
fn validator_rejects_data_config_with_missing_mqtt_sink() {
    let package = EdgeConfigPackage::new("edge-dev", "v1")
        .with_device(DeviceInstance::new("pump-1", "pump"))
        .with_protocol_connection(ProtocolConnection::simulated("sim-line"))
        .with_data_config(DataConfig::new(
            "pump_status",
            "泵运行状态上报",
            "pump-1",
            "sim-line",
            DataConfigCollection::new(1000),
            DataConfigPublish::new(
                "missing-sink",
                "factory/{edge_id}/{device_id}/status",
                DataConfigPayload::object(),
            ),
        ));

    let error = validate_edge_config_package(&package).unwrap_err();
    assert!(error.to_string().contains("missing-sink"));
}
```

- [ ] **Step 2: Implement validation**

In `validation.rs`, add checks:

```rust
for data_config in &package.data_configs {
    require_non_empty(&data_config.config_id, "data config id")?;
    require_non_empty(&data_config.device_id, "data config device id")?;
    require_non_empty(&data_config.protocol_connection_id, "data config protocol connection")?;
    if !mqtt_sink_ids.contains(&data_config.publish.sink_id) {
        bail!("data config {} references missing mqtt sink {}", data_config.config_id, data_config.publish.sink_id);
    }
    if !device_ids.contains(&data_config.device_id) {
        bail!("data config {} references missing device {}", data_config.config_id, data_config.device_id);
    }
    if !connection_ids.contains(&data_config.protocol_connection_id) {
        bail!("data config {} references missing protocol connection {}", data_config.config_id, data_config.protocol_connection_id);
    }
    if data_config.points.is_empty() {
        bail!("data config {} must contain at least one point", data_config.config_id);
    }
}
```

- [ ] **Step 3: Add API tests**

Add to `crates/cloud-api/tests/api.rs`:

```rust
#[tokio::test]
async fn data_config_endpoints_create_and_list_edge_configs() {
    let router = test_router().await;

    let response = router
        .clone()
        .oneshot(
            Request::post("/api/edges/edge-dev/data-configs")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "configId": "pump_status",
                        "name": "泵运行状态上报",
                        "enabled": true,
                        "deviceId": "pump-1",
                        "protocolConnectionId": "modbus-line-a",
                        "collection": {"periodMs": 1000, "timeoutMs": 800, "retryCount": 2},
                        "points": [{
                            "pointId": "pressure",
                            "semanticId": "pump.pressure",
                            "addressKind": "holding_register",
                            "addressValue": "40001",
                            "valueType": "float32",
                            "unit": "MPa",
                            "jsonField": "pressure"
                        }],
                        "publish": {
                            "sinkId": "velamq-main",
                            "topicTemplate": "factory/{edge_id}/{device_id}/status",
                            "qos": 1,
                            "payload": {"mode": "object", "timestampField": "ts", "includeQuality": true}
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = router
        .oneshot(Request::get("/api/edges/edge-dev/data-configs").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let configs: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(configs.as_array().unwrap().iter().any(|config| config["configId"] == "pump_status"));
}
```

- [ ] **Step 4: Implement API DTOs and handlers**

In `api.rs`, add request/response structs using camelCase fields and handlers for:

```rust
GET /api/edges/{edge_id}/data-configs
POST /api/edges/{edge_id}/data-configs
PUT /api/edges/{edge_id}/data-configs/{config_id}
DELETE /api/edges/{edge_id}/data-configs/{config_id}
```

Handlers should modify the selected edge draft package and call the same release-list refresh path used by point/task saves.

- [ ] **Step 5: Run tests**

```bash
cargo test -p cloud-control validator_rejects_data_config_with_missing_mqtt_sink
cargo test -p cloud-api data_config_endpoints_create_and_list_edge_configs
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/cloud-control/src/validation.rs crates/cloud-control/tests/config_loop.rs crates/cloud-api/src/api.rs crates/cloud-api/src/state.rs crates/cloud-api/tests/api.rs
git commit -m "feat: add data config cloud api"
```

---

### Task 5: Console Data Config API Client

**Files:**
- Modify: `web/console/src/api/types.ts`
- Modify: `web/console/src/api/client.ts`
- Test: `web/console/src/api/client.test.ts`

- [ ] **Step 1: Add client test**

Add:

```ts
it('creates and lists edge data configs', async () => {
  mockFetch
    .mockResolvedValueOnce(jsonResponse([
      {
        configId: 'pump_status',
        name: '泵运行状态上报',
        enabled: true,
        deviceId: 'pump-1',
        protocolConnectionId: 'modbus-line-a',
        collection: { periodMs: 1000, timeoutMs: 800, retryCount: 2 },
        points: [],
        publish: {
          sinkId: 'velamq-main',
          topicTemplate: 'factory/{edge_id}/{device_id}/status',
          qos: 1,
          payload: { mode: 'object', timestampField: 'ts', includeQuality: true },
        },
      },
    ]))
    .mockResolvedValueOnce(jsonResponse({ configId: 'pump_status' }, 201));

  const configs = await fetchEdgeDataConfigs('edge-dev');
  expect(configs[0].configId).toBe('pump_status');

  await createEdgeDataConfig('edge-dev', configs[0]);
  expect(mockFetch).toHaveBeenLastCalledWith(
    '/api/edges/edge-dev/data-configs',
    expect.objectContaining({ method: 'POST' }),
  );
});
```

- [ ] **Step 2: Add TS types**

In `types.ts`, add:

```ts
export interface DataConfigResponse {
  configId: string;
  name: string;
  enabled: boolean;
  deviceId: string;
  protocolConnectionId: string;
  collection: DataConfigCollection;
  points: DataConfigPoint[];
  publish: DataConfigPublish;
}
```

Add corresponding nested interfaces and reuse them for create/save requests.

- [ ] **Step 3: Add client methods**

In `client.ts`, add:

```ts
export async function fetchEdgeDataConfigs(edgeId: string) {
  return request<DataConfigResponse[]>(`/api/edges/${edgeId}/data-configs`);
}

export async function createEdgeDataConfig(edgeId: string, requestBody: SaveDataConfigRequest) {
  return request<DataConfigResponse>(`/api/edges/${edgeId}/data-configs`, {
    method: 'POST',
    body: JSON.stringify(requestBody),
  });
}

export async function saveEdgeDataConfig(edgeId: string, configId: string, requestBody: SaveDataConfigRequest) {
  return request<DataConfigResponse>(`/api/edges/${edgeId}/data-configs/${configId}`, {
    method: 'PUT',
    body: JSON.stringify(requestBody),
  });
}
```

- [ ] **Step 4: Run tests**

```bash
npm test -- --run src/api/client.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/console/src/api/types.ts web/console/src/api/client.ts web/console/src/api/client.test.ts
git commit -m "feat: add data config console client"
```

---

### Task 6: Console Data Config Page

**Files:**
- Create: `web/console/src/pages/DataConfigsPage.tsx`
- Create: `web/console/src/pages/DataConfigsPage.test.tsx`
- Modify: `web/console/src/pages/PointMappingsPage.css`

- [ ] **Step 1: Write page test**

Create `DataConfigsPage.test.tsx`:

```tsx
import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { DataConfigsPage } from './DataConfigsPage';

describe('DataConfigsPage', () => {
  it('opens step dialog and saves a complete data config', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);

    render(
      <DataConfigsPage
        configs={[]}
        edges={[{ edgeId: 'edge-dev', displayName: '研发实验室边端' } as any]}
        protocolConnections={[{ connectionId: 'modbus-line-a', protocol: 'Modbus RTU' } as any]}
        mqttUplink={{ sinkId: 'velamq-main' } as any}
        selectedEdgeId="edge-dev"
        onSaveConfig={onSave}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '新建数据配置' }));
    const dialog = screen.getByRole('dialog', { name: '新建数据配置' });
    fireEvent.change(within(dialog).getByLabelText('配置 ID'), { target: { value: 'pump_status' } });
    fireEvent.change(within(dialog).getByLabelText('配置名称'), { target: { value: '泵运行状态上报' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '下一步' }));

    fireEvent.change(within(dialog).getByLabelText('采集周期(ms)'), { target: { value: '1000' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '下一步' }));

    fireEvent.change(within(dialog).getByLabelText('Point ID'), { target: { value: 'pressure' } });
    fireEvent.change(within(dialog).getByLabelText('地址值'), { target: { value: '40001' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '下一步' }));

    fireEvent.change(within(dialog).getByLabelText('MQTT Topic'), {
      target: { value: 'factory/{edge_id}/{device_id}/status' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '下一步' }));

    expect(within(dialog).getByLabelText('JSON 预览')).toHaveTextContent('pressure');
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    expect(onSave).toHaveBeenCalledWith('edge-dev', expect.objectContaining({
      configId: 'pump_status',
      collection: expect.objectContaining({ periodMs: 1000 }),
    }));
  });
});
```

- [ ] **Step 2: Implement page**

Create `DataConfigsPage.tsx` with:

- List table of configs.
- `新建数据配置` button.
- Dialog state: `step`, `form`.
- Five step sections.
- JSON preview generated from `form.points`.
- Save handler.

Use existing `modal-backdrop`, `modal-panel`, `ops-table`, `editor-control`, and `primary-button` classes.

- [ ] **Step 3: Run page test**

```bash
npm test -- --run src/pages/DataConfigsPage.test.tsx
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add web/console/src/pages/DataConfigsPage.tsx web/console/src/pages/DataConfigsPage.test.tsx web/console/src/pages/PointMappingsPage.css
git commit -m "feat: add data config console page"
```

---

### Task 7: App Integration And Navigation

**Files:**
- Modify: `web/console/src/layout/AppShell.tsx`
- Modify: `web/console/src/App.tsx`
- Modify: `web/console/src/App.test.tsx`

- [ ] **Step 1: Update App test**

Add/modify an integration test:

```tsx
it('uses data configuration as the primary data flow page', async () => {
  render(<App />);

  expect(screen.getByRole('button', { name: /数据配置/ })).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /点位配置/ })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /采集任务/ })).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole('button', { name: /数据配置/ }));
  expect(await screen.findByRole('heading', { name: '数据配置', level: 2 })).toBeInTheDocument();
});
```

- [ ] **Step 2: Update navigation**

In `AppShell.tsx`, replace point/task nav entries with:

```ts
{ key: 'dataConfigs', label: '数据配置', icon: ListChecks }
```

Rename `MQTT 上报` label to `MQTT Sink`.

- [ ] **Step 3: Wire state and handlers**

In `App.tsx`:

- Add `dataConfigs` state to `ConsoleSnapshot`.
- Fetch data configs in `loadConsoleSnapshot`.
- Render `DataConfigsPage` for `activePage === 'dataConfigs'`.
- Add `handleSaveDataConfig` using `createEdgeDataConfig` or `saveEdgeDataConfig`.

- [ ] **Step 4: Run integration test**

```bash
npm test -- --run src/App.test.tsx
```

Expected: PASS.

- [ ] **Step 5: Build frontend**

```bash
npm run build
```

Expected: Vite build succeeds and `web/console/dist` updates.

- [ ] **Step 6: Commit**

```bash
git add web/console/src/layout/AppShell.tsx web/console/src/App.tsx web/console/src/App.test.tsx web/console/dist
git commit -m "feat: integrate data config console flow"
```

---

## Final Verification

- [ ] Run all Rust tests:

```bash
cargo fmt && cargo test
```

Expected: all Rust tests pass.

- [ ] Run all frontend tests and build:

```bash
cd web/console
npm test -- --run
npm run build
```

Expected: all Vitest tests pass and Vite build succeeds.

- [ ] Browser smoke:

```text
Open http://127.0.0.1:8080/
Click 数据配置
Click 新建数据配置
Fill one Modbus point and MQTT topic
Reach JSON 预览
Save
Confirm the new data config appears in the list
```

Expected: the UI works without static placeholder-only behavior.

---

## Self-Review

- Spec coverage: core model, runtime, cloud API, console page, validation, storage-by-package, and tests are covered.
- Placeholder scan: no TBD/TODO placeholders remain.
- Type consistency: plan uses `DataConfig`, `DataConfigPoint`, `DataConfigCollection`, `DataConfigPublish`, and `DataConfigPayload` consistently across Rust and TypeScript.
