export interface SummaryResponse {
  edge_count: number;
  pending_release_count: number;
}

export interface PointMappingResponse {
  edgeId: string;
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

export interface CreatePointMappingRequest {
  pointId?: string;
  deviceId?: string;
  semanticId?: string;
  connectionId?: string;
  addressKind?: string;
  addressValue?: string;
  valueType?: string;
  unit?: string;
  intervalMs?: number;
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

export interface CreateEdgeNodeRequest {
  displayName: string;
  site: string;
}

export interface EdgeNodeActionResponse {
  action: string;
  credentialVersion?: string;
  edgeId: string;
  message: string;
  status?: string;
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

export interface CreateTelemetryModelRequest {
  telemetryId: string;
  valueType: string;
  unit: string;
  range: string;
  description: string;
}

export interface CreateDeviceModelRequest {
  deviceType: string;
  version: string;
  telemetry: CreateTelemetryModelRequest[];
}

export interface SaveDeviceModelRequest {
  version: string;
  telemetry: CreateTelemetryModelRequest[];
}

export interface ManagementActionResponse {
  action: string;
  details: string[];
  message: string;
  status: string;
}

export interface AgentSuggestionResponse {
  title: string;
  detail: string;
  state: string;
}

export interface AgentActionResponse extends ManagementActionResponse {
  suggestions: AgentSuggestionResponse[];
}

export interface ProtocolConnectionResponse {
  edgeId: string;
  connectionId: string;
  protocolType: string;
  protocol: string;
  endpoint: string;
  status: string;
  policy: string;
}

export interface SaveProtocolConnectionRequest {
  protocolType: string;
  endpoint: string | null;
}

export interface CreateProtocolConnectionRequest {
  protocolType: string;
  endpoint: string | null;
}

export interface CollectionTaskResponse {
  edgeId: string;
  taskId: string;
  deviceId: string;
  pointIds: string[];
  pointList: string;
  intervalMs: number;
  interval: string;
  enabled: boolean;
  status: string;
}

export interface SaveCollectionTaskRequest {
  deviceId: string;
  pointIds: string[];
  intervalMs: number;
  enabled: boolean;
}

export interface CreateCollectionTaskRequest {
  taskId?: string;
  deviceId: string;
  pointIds: string[];
  intervalMs: number;
  enabled?: boolean;
}

export interface AlgorithmResponse {
  edgeId: string;
  algorithmId: string;
  version: string;
  algorithmKind: AlgorithmKind;
  dsl: AlgorithmDsl;
  runtime: string;
  kind: string;
  inputIds: string[];
  outputIds: string[];
  inputs: string;
  outputs: string;
  execution: string;
  validation: string;
}

export type AlgorithmKind =
  | 'ChangeReport'
  | 'WindowAggregate'
  | 'ExpressionAggregate'
  | 'ThresholdRule'
  | 'DurationRule'
  | 'Deadband'
  | 'Debounce'
  | 'Statistics';

export interface AlgorithmDsl {
  inputs: AlgorithmInputBinding[];
  trigger: AlgorithmTrigger;
  steps: AlgorithmStep[];
  outputs: AlgorithmOutput[];
  report: AlgorithmReportPolicy;
}

export interface AlgorithmInputBinding {
  alias: string;
  pointId: string;
}

export type AlgorithmTrigger =
  | { type: 'onSample' }
  | { type: 'onAnyInput' }
  | { type: 'window'; everyMs: number };

export type AlgorithmStep =
  | { type: 'changeFilter'; source: string; threshold: number }
  | {
      type: 'windowAggregate';
      source: string;
      functions: Array<{ function: 'avg' | 'min' | 'max' | 'sum' | 'count'; output: string }>;
    }
  | { type: 'expression'; output: string; expr: string }
  | {
      type: 'thresholdRule';
      source: string;
      operator: 'Gt' | 'Gte' | 'Lt' | 'Lte' | 'Eq' | 'Ne';
      threshold: number;
      event: { code: string; severity: 'Info' | 'Warning' | 'Critical'; message: string };
    };

export interface AlgorithmOutput {
  name: string;
  pointId: string;
}

export interface AlgorithmReportPolicy {
  mode: 'OnOutput' | 'OnChange' | 'WindowResult' | 'EventOnly';
  sink: string;
}

export interface SaveAlgorithmRequest {
  version: string;
  algorithmKind: AlgorithmKind;
  dsl: AlgorithmDsl;
}

export interface CreateAlgorithmRequest extends SaveAlgorithmRequest {
  algorithmId?: string;
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

export interface MqttUplinkResponse {
  sinkId: string;
  broker: string;
  clientId: string;
  topicTemplate: string;
  qos: number;
  batchSize: number;
  flushIntervalMs: number;
}

export type SaveMqttUplinkRequest = MqttUplinkResponse;

export interface RunDiscoveryRequest {
  connectionId: string;
  addressRange: string;
}

export interface DiscoveredPointResponse {
  protocolConnectionId: string;
  address: string;
  valueType: string;
  sampleValues: string[];
  confidence: number;
}

export interface PointMappingSuggestionResponse {
  pointId: string;
  deviceId: string;
  semanticId: string;
  protocolConnectionId: string;
  address: string;
  valueType: string;
  unit: string;
  confidence: number;
  evidence: string;
}

export interface DiscoveryReportResponse {
  jobId: string;
  protocolConnectionId: string;
  discoveredPoints: DiscoveredPointResponse[];
  suggestions: PointMappingSuggestionResponse[];
}
