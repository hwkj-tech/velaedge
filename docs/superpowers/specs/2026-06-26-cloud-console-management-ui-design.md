# Cloud Console Management UI Design

## Goal

Design the first version of the cloud management console for the edge-cloud platform. The console focuses on one complete operational loop:

```text
Register edge node
-> define device model
-> configure protocol connection
-> configure telemetry points
-> create collection task
-> generate config version
-> publish to edge runtime
-> observe apply result and collection status
```

This design is the approval baseline before implementation. The Figma file was created at `https://www.figma.com/design/c4U10VbV37cXx0lGT0TOJw`, but the current Figma Starter plan hit the MCP write limit before screens could be generated. Until Figma write access is available again, this document is the source of truth for the first UI version.

## Product Positioning

The console is an engineering operations tool, not a marketing dashboard. It should prioritize dense but readable information, predictable navigation, high-confidence publishing, auditability, and fast repeated configuration work.

Target users:

- Platform administrators who register edge nodes and manage rollout policy.
- Device engineers who define device models, protocol mappings, and telemetry points.
- Operations engineers who publish configurations and inspect collection/runtime status.
- Maintenance engineers who use Agent-assisted diagnostics without bypassing safety controls.

## Architecture Boundary

The UI follows the platform's cloud-edge-device boundary:

- Cloud console owns configuration authoring, versioning, validation, release planning, audit views, and Agent-assisted drafts.
- Edge runtime owns protocol adapters, actual device connection, local policy checks, local shadow, local buffering, and collection execution.
- Devices own real physical action, firmware or PLC logic, and hard interlocks.
- Agent features may generate drafts, explanations, and risk analysis. They do not publish configurations or execute device commands without user approval.

## Navigation

The console uses a left navigation, top status bar, and main work area.

```text
EdgeOps Cloud
├─ 工作台
├─ 边端管理
├─ 设备模型
├─ 协议连接
├─ 点位配置
├─ 采集任务
├─ 算法配置
├─ 配置发布
├─ 运行状态
├─ 审计日志
└─ Agent 助手
```

The top bar displays:

- Current tenant/project.
- Current environment.
- Online edge count.
- Draft configuration version.
- Latest release status.
- User/account menu.

## Page Designs

### 工作台

Purpose: provide a fleet-level operational summary and entry points to pending work.

Key modules:

- Metric strip: total edge nodes, online rate, telemetry point count, abnormal points, pending releases, high-risk alerts.
- Edge health list: edge id, site, online status, current config version, reported config version, last heartbeat.
- Configuration loop panel: draft, validation, release, edge apply, status return.
- Recent events: connection failures, config apply failures, policy rejections, algorithm alarms.
- Quick actions: register edge, create point, publish config, inspect failed edge.

### 边端管理

Purpose: manage edge node lifecycle and observe edge runtime readiness.

Main table columns:

- Edge ID.
- Display name.
- Site/group.
- Runtime version.
- Online status.
- Desired config version.
- Reported config version.
- CPU/memory/disk summary.
- Last heartbeat.
- Actions.

Edge detail drawer:

- Basic metadata.
- Runtime capabilities.
- Protocol adapters installed.
- Local storage status.
- Cloud sync status.
- Certificate status.
- Recent config apply history.

Primary actions:

- Register edge node.
- Assign to group.
- Rotate credential.
- Mark maintenance mode.
- View runtime logs.

### 设备模型

Purpose: define semantic device types before protocol-specific point mappings.

Model fields:

- Device type.
- Version.
- Telemetry definitions.
- Command definitions.
- Event definitions.
- Units, ranges, data types, and descriptions.

Important rule: semantic telemetry is separate from protocol mapping. For example, `pressure` is a semantic telemetry id. Its Modbus register or OPC UA NodeId is configured in 点位配置.

### 协议连接

Purpose: define reusable protocol connection instances that point mappings and collection tasks can reference.

Supported first version protocol types:

- Simulated.
- Modbus TCP.
- OPC UA.
- MQTT.
- Siemens S7 placeholder.

Connection fields:

- Connection ID.
- Protocol type.
- Host/port or broker URL.
- Security mode.
- Timeout.
- Retry policy.
- Polling limits.
- Enable/disable status.

Validation:

- Required fields by protocol type.
- Connection ID uniqueness.
- Secret fields must not be exposed in clear text after save.

### 点位配置

Purpose: configure telemetry point mappings from semantic points to protocol addresses.

This is the core page for first development.

Layout:

```text
Left navigation
Top status bar
Main table: telemetry point mappings
Right drawer: point editor
Footer/modal: import and validation result
```

Table columns:

- Point ID.
- Point name.
- Device ID.
- Semantic telemetry.
- Protocol.
- Connection.
- Address / NodeId / Topic.
- Data type.
- Read/write type.
- Unit.
- Scale.
- Collection interval.
- Range.
- Quality rule.
- Status.

Editor drawer sections:

Basic information:

- Point ID.
- Display name.
- Device ID.
- Device model.
- Semantic telemetry ID.
- Enable/disable status.

Protocol mapping:

- Protocol type.
- Connection instance.
- Address type.
- Address, NodeId, DB address, or MQTT topic.
- Data type.
- Byte order.
- Bit offset when applicable.
- Scale.
- Offset.

Collection policy:

- Collection interval.
- Timeout.
- Retry count.
- Deadband.
- Cache policy.

Data governance:

- Unit.
- Min value.
- Max value.
- Precision.
- Quality rules.
- Alarm rule reference.

Examples:

```text
pressure
protocol = Modbus TCP
address_type = holding_register
address = 40001
data_type = float32
scale = 0.01
unit = MPa
range = 0..20
interval = 1000ms
```

```text
temperature
protocol = OPC UA
node_id = ns=2;s=Pump.Temperature
data_type = float
unit = Celsius
interval = 1000ms
```

### 采集任务

Purpose: bind devices, protocol connections, and point groups into runtime collection work.

Fields:

- Task ID.
- Target edge group or edge id.
- Device instance.
- Point set.
- Schedule mode: interval, cron, stream.
- Default collection interval.
- Local buffering policy.
- Enable/disable status.

The first version can support interval collection only. The page should still reserve schedule mode so cron/stream can be added later without redesign.

### 算法配置

Purpose: configure edge-side algorithm packages and their point bindings.

Fields:

- Algorithm ID.
- Version.
- Runtime: rule, WASM, ONNX, Python.
- Inputs: telemetry points.
- Outputs: derived telemetry or events.
- Schedule: stream, interval, cron.
- Resource limits.
- Whether command output is allowed.

First version behavior:

- Support visual configuration and version packaging.
- Do not execute real algorithms beyond simulated metadata.

### 配置发布

Purpose: safely publish versioned configuration to edge runtimes.

Release flow:

```text
Draft config
-> validation
-> version creation
-> diff review
-> target edge selection
-> publish
-> edge download
-> edge apply
-> reported version return
-> success, failure, or rollback
```

Layout:

Left panel:

- Version number.
- Change summary.
- Risk level.
- Validation result.
- Affected edges.
- Affected devices.
- Affected points.
- Affected algorithms.

Right panel:

- Edge ID.
- Desired version.
- Reported version.
- Download status.
- Apply status.
- Error reason.
- Rollback action.

Validation rules:

- No duplicate point ID in the same edge scope.
- Every point references an existing device, semantic telemetry, and protocol connection.
- Collection interval must be within allowed bounds.
- Numeric range min must be less than max.
- Address format must match protocol type.
- High-risk changes require explicit confirmation.

### 运行状态

Purpose: inspect whether runtime execution matches cloud intent.

Views:

- Edge runtime status.
- Device connection status.
- Point collection status.
- Latest telemetry values.
- Local cache queue.
- Device shadow.
- Config apply history.

Important comparison:

```text
desired_config_version vs reported_config_version
```

If they differ, the UI must show whether the edge is downloading, validating, applying, failed, or waiting for connectivity.

### 审计日志

Purpose: provide a complete audit trail for compliance and operations.

Log types:

- Edge registration.
- Device model changes.
- Protocol connection changes.
- Point mapping changes.
- Collection task changes.
- Algorithm config changes.
- Config version creation.
- Config publish.
- Edge apply result.
- Rollback.
- Agent suggestion accepted or rejected.

Columns:

- Time.
- Actor.
- Action.
- Target.
- Before version.
- After version.
- Result.
- Reason.

### Agent 助手

Purpose: make configuration and operations faster while preserving human control.

Agent entry points:

- 点位配置: generate point draft from device manual or point table.
- 配置发布: explain diff and detect risky changes.
- 运行状态: analyze collection failures and likely root causes.
- 算法配置: recommend input and output point mappings.

Rules:

- Agent output is a draft or explanation.
- User approval is required before saving or publishing.
- Edge runtime still performs final validation and policy checks.

## Core Data Model For UI

The management UI should produce and display these cloud-side objects:

- `EdgeNode`
- `DeviceModel`
- `DeviceInstance`
- `ProtocolConnection`
- `TelemetryPointMapping`
- `CollectionTask`
- `AlgorithmBinding`
- `ConfigDraft`
- `ConfigVersion`
- `ReleasePlan`
- `ReleaseResult`
- `AuditRecord`

These objects compile into the edge-facing `ConfigPackage`.

## Configuration Package Shape

The console should eventually generate a package equivalent to:

```yaml
edge_id: edge-dev
version: 2026.06.26-001
devices:
  - id: pump-1
    model: pump
protocol_connections:
  - id: modbus-main
    type: modbus_tcp
    host: 192.168.1.10
    port: 502
point_mappings:
  - point_id: pressure
    device_id: pump-1
    semantic_id: pressure
    protocol_connection: modbus-main
    address_type: holding_register
    address: "40001"
    data_type: float32
    scale: 0.01
    unit: MPa
    interval_ms: 1000
collection_tasks:
  - id: pump-main-collection
    device_id: pump-1
    points: [pressure]
    interval_ms: 1000
algorithms:
  - id: pump-anomaly
    version: 1.0.0
    runtime: onnx
    inputs: [pressure]
```

## Visual Direction

Style: industrial SaaS console.

Layout:

- Left sidebar navigation.
- Top status bar.
- Dense main content area.
- Tables for repeated operational data.
- Right drawers for editing.
- Modals for import, validation result, and dangerous publish confirmation.

Color use:

- Neutral light gray background.
- White panels.
- Blue for primary actions and config versioning.
- Green for online/success.
- Amber for sync/warning.
- Red for failure/high risk.

Component preferences:

- Tables for point, edge, and release result data.
- Drawer for edit forms.
- Tabs for detail pages.
- Status tags for online/sync/fail states.
- Stepper for release flow.
- Diff viewer for config release.
- Filter bars for large lists.

Avoid:

- Marketing hero layouts.
- Oversized decorative cards.
- Pure dark dashboard style for this first version.
- Hiding important configuration fields behind too many wizard steps.

## First Version Acceptance Criteria

The first implementation should satisfy:

- User can register an edge node.
- User can create a device model.
- User can create a protocol connection.
- User can create telemetry point mappings.
- User can create an interval collection task.
- User can generate a config version.
- User can simulate publishing the config to an edge runtime.
- Edge runtime can apply the simulated package and collect simulated telemetry.
- UI shows desired version, reported version, apply status, and latest collection status.
- Configuration changes and release operations create audit records.

## Deferred Scope

Do not implement these in the first UI version:

- Full multi-tenant permission matrix.
- Real visual topology map.
- Real ONNX/WASM algorithm execution.
- Real Modbus/OPC UA/S7/FINS protocol drivers.
- Full document ingestion and RAG.
- Automatic remediation by Agent.

The first release must make the configuration loop reliable before adding higher-level intelligence.

