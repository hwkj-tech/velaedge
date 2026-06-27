# Cloud Console Workflow

The cloud console is the first built-in management surface for the edge-cloud platform. It is an operations tool for configuring edge collection, releasing versioned config packages, and checking edge apply status.

## Run Locally

Build the console and serve it from Rust:

```bash
cd web/console
npm install
npm run build
cd ../..
cargo run -p cloud-api
```

Open `http://127.0.0.1:8080`.

For frontend-only iteration:

```bash
cd web/console
npm run dev
```

Cloud metadata is stored in SQLite by the cloud service. Runtime-local desired config, queues, and applied-state records are designed for RocksDB on the edge side.

## Configuration Loop

1. Edge registration
   Register edge nodes, assign site or group metadata, track runtime version, heartbeat, desired config version, and reported config version.

2. Device model
   Define semantic telemetry, commands, events, units, ranges, and data types before choosing protocol-specific addresses.

3. Protocol connection
   Create reusable southbound collection connections such as Simulated, Modbus RTU, DL/T645, IEC 101, or custom serial adapters. Secret fields must be governed and not displayed in clear text after save.

4. Point mapping
   Map a semantic telemetry id to a protocol connection and address, for example `voltage_a -> Modbus RTU -> holding_register:40001`. Configure data type, scale, unit, interval, quality rule, and range.

5. Point discovery
   Run controlled read-only serial discovery from cloud. Runtime reports discovered addresses and samples, then the Agent produces point mapping suggestions for user confirmation.

6. Collection task
   Group point mappings into runtime tasks with interval, timeout, retry, deadband, and cache policy.

7. Algorithm configuration
   Attach local edge algorithms to selected input points. The first UI models rule, aggregation, and anomaly-detection templates. Production runners can later support WASM or ONNX packages.

8. MQTT uplink
   Configure the runtime's northbound MQTT publishing sink to velaMQ. MQTT is used for serial telemetry upload, not as a device-side acquisition protocol.

9. Config release
   Generate an `EdgeConfigPackage`, validate references, review change summary, and publish to selected edge nodes.

10. Simulated apply
   The current runtime path validates and applies config packages locally, records applied version, and can produce simulated telemetry for the configured points.

11. Runtime status
   Compare desired and reported versions, heartbeat, protocol adapter capability, local storage status, and config apply history.

## Page Map

- 工作台: fleet summary, health list, recent events, and quick actions.
- 边端管理: edge lifecycle, credentials, maintenance mode, and runtime capability.
- 设备模型: semantic telemetry, commands, events, units, ranges, and types.
- 协议连接: serial collection connection list and protocol-specific validation.
- 点位配置: primary point table and right-side editor drawer.
- 采集任务: runtime collection scheduling.
- 算法配置: edge-local algorithm templates and input mappings.
- MQTT 上报: velaMQ broker, client id, topic template, QoS, batch, and flush policy.
- 点位探测: controlled serial discovery and Agent mapping suggestions.
- 配置发布: validation, change summary, desired versions, reported versions, and apply results.
- 运行状态: runtime health and capability reporting.
- 审计日志: immutable trail of drafts, validation, release, and apply records.
- Agent 助手: advisory drafts, explanations, and risk analysis.

## Agent Boundary

Agent capabilities are intentionally advisory:

- It may draft point mappings, explain failures, summarize release risk, or suggest rollout plans.
- It must not publish configurations without user approval.
- It must not directly write protocol registers or execute physical device commands.
- Any command candidate must pass cloud governance and edge-side policy validation.

This boundary keeps intelligence useful without weakening deterministic runtime safety.
