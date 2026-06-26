export interface SummaryResponse {
  edge_count: number;
  pending_release_count: number;
}

export interface PointMappingResponse {
  pointId: string;
  pointName: string;
  deviceId: string;
  deviceModel: string;
  semanticTelemetry: string;
  protocol: string;
  connection: string;
  address: string;
  valueType: string;
  readWrite: string;
  unit: string;
  scale: string;
  interval: string;
  range: string;
  qualityRule: string;
  status: string;
}

export interface SavePointMappingRequest {
  addressKind: string;
  addressValue: string;
  intervalMs: number;
  unit: string;
}

export interface ApplyResultResponse {
  edgeId: string;
  desiredVersion: string;
  reportedVersion: string;
  result: string;
  heartbeat: string;
}

export interface ReleaseListResponse {
  draftVersion: string;
  validationStatus: string;
  changeSummary: string;
  rolloutPolicy: string;
  applyResults: ApplyResultResponse[];
}

export interface EdgeNodeResponse {
  edgeId: string;
  displayName: string;
  site: string;
  runtimeId: string;
  status: string;
  resources: string;
  heartbeat: string;
  capabilities: string[];
}

export interface TelemetryModelResponse {
  telemetryId: string;
  name: string;
  valueType: string;
  unit: string;
  range: string;
  description: string;
}

export interface DeviceModelResponse {
  deviceType: string;
  version: string;
  telemetry: TelemetryModelResponse[];
  commandCount: number;
  eventCount: number;
}

export interface ProtocolConnectionResponse {
  edgeId: string;
  connectionId: string;
  protocol: string;
  endpoint: string;
  status: string;
  policy: string;
}

export interface CollectionTaskResponse {
  edgeId: string;
  taskId: string;
  deviceId: string;
  pointList: string;
  interval: string;
  status: string;
}

export interface AlgorithmResponse {
  edgeId: string;
  algorithmId: string;
  version: string;
  kind: string;
  inputs: string;
  outputs: string;
  execution: string;
  validation: string;
}

export interface AuditRecordResponse {
  createdAt: string;
  time: string;
  actor: string;
  action: string;
  target: string;
  result: string;
}

export type EdgeHealth = 'Healthy' | 'Degraded' | 'Critical' | 'Offline';

export interface SystemRuntimeMetrics {
  cpu_percent: number;
  memory_percent: number;
  disk_percent: number;
  process_uptime_seconds: number;
}

export interface CollectionRuntimeMetrics {
  active_task_count: number;
  success_rate: number;
  average_latency_ms: number;
  bad_point_count: number;
}

export interface ProtocolRuntimeMetrics {
  connection_id: string;
  protocol: string;
  connected: boolean;
  latency_ms: number;
  timeout_count: number;
  error_count: number;
  reconnect_count: number;
}

export interface LocalStoreMetrics {
  backend: string;
  buffered_records: number;
  oldest_buffer_age_seconds: number;
  disk_usage_percent: number;
}

export interface AlgorithmRuntimeMetrics {
  algorithm_id: string;
  healthy: boolean;
  last_run_latency_ms: number;
  error_count: number;
  alert_count: number;
}

export interface CloudSyncMetrics {
  connected: boolean;
  last_sync_seconds_ago: number;
  pending_uploads: number;
  desired_version: string;
  reported_version: string;
}

export interface EdgeRuntimeMetricsSnapshot {
  edge_id: string;
  runtime_id: string;
  config_version: string;
  timestamp: string;
  health: EdgeHealth;
  system: SystemRuntimeMetrics;
  collection: CollectionRuntimeMetrics;
  protocols: ProtocolRuntimeMetrics[];
  local_store: LocalStoreMetrics;
  algorithms: AlgorithmRuntimeMetrics[];
  cloud_sync: CloudSyncMetrics;
}

export type RuntimeEventSeverity = 'Info' | 'Warning' | 'Critical';
export type RuntimeEventCategory =
  | 'System'
  | 'Protocol'
  | 'Collection'
  | 'Storage'
  | 'Algorithm'
  | 'Sync';

export interface EdgeRuntimeEvent {
  edge_id: string;
  severity: RuntimeEventSeverity;
  category: RuntimeEventCategory;
  code: string;
  message: string;
  timestamp: string;
  context: Record<string, string>;
}

export interface RuntimeStatusResponse {
  healthyEdgeCount: number;
  degradedEdgeCount: number;
  criticalEdgeCount: number;
  averageCollectionLatencyMs: number;
  edges: EdgeRuntimeMetricsSnapshot[];
  events: EdgeRuntimeEvent[];
}
