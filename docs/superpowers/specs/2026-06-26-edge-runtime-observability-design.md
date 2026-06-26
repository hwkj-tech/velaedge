# Edge Runtime Observability Design

## Goal

Add a real-time monitoring foundation for edge runtime health. Edge programs report system and runtime metrics to the cloud, and the cloud console shows current edge health, resource usage, collection quality, protocol status, local storage pressure, algorithm behavior, and cloud sync state.

## Architecture Boundaries

- Edge runtime owns local metric collection, local event generation, and offline buffering.
- Cloud control owns latest-status storage, history-ready models, fleet-level health aggregation, alert classification, and API exposure.
- Cloud console owns operator-facing live status, edge detail inspection, and event visibility.
- Agent services may explain anomalies and suggest remediation, but they do not replace deterministic metric collection or health classification.

## MVP Scope

The first implementation focuses on a deterministic single-edge loop:

- Shared `edge-core` models for `EdgeRuntimeMetricsSnapshot`, `EdgeRuntimeEvent`, and nested metric groups.
- Edge runtime simulated metrics collector that produces a realistic snapshot from an applied config/runtime state.
- Cloud control store methods for latest runtime snapshots and recent events.
- Cloud API endpoints to receive metrics/events and query current runtime status.
- Console `RuntimeStatusPage` backed by the API instead of static rows.

## Runtime Metrics Model

The snapshot uses camel-case JSON for frontend/API responses and snake-case Rust field names:

- `edge_id`, `runtime_id`, `config_version`, `timestamp`, `health`
- `system`: CPU percent, memory percent, disk percent, process uptime seconds
- `collection`: active task count, success rate, average latency, bad point count
- `protocols`: per-connection id/protocol/status/latency/error counters
- `local_store`: backend, buffered records, oldest buffered age, disk usage percent
- `algorithms`: algorithm id/status/last run latency/error count
- `cloud_sync`: connected flag, last sync seconds ago, pending uploads, desired/reported version

Runtime events include severity, category, code, message, timestamp, and context key/value fields.

## Cloud API

MVP endpoints:

- `POST /api/edges/{edge_id}/runtime-metrics`: upsert latest snapshot for the edge.
- `POST /api/edges/{edge_id}/runtime-events`: append one event for the edge.
- `GET /api/runtime-status`: return fleet summary plus latest per-edge status and recent events.

The API should reject reports whose path `edge_id` does not match the payload edge id.

## Console Experience

The runtime page should show:

- Summary tiles: online/healthy edges, degraded edges, critical edges, average collection latency.
- Edge table: edge id, health, CPU, memory, disk, config version, sync status, heartbeat.
- Protocol table: selected/latest edge protocol connections with status, latency, timeout count.
- Event stream: recent warning/critical/info events.

The existing static capabilities table is replaced by API-backed runtime status data.

## Validation

Tests should prove:

- Core metrics/event models serialize and preserve health/runtime fields.
- Edge runtime simulated collector emits a snapshot aligned with an applied config.
- Cloud store upserts latest runtime status and keeps recent runtime events.
- Cloud API accepts reports and serves runtime status.
- Console client and page render API-backed runtime status.
