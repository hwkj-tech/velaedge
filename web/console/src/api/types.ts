export interface SummaryResponse {
  edge_count: number;
  pending_release_count: number;
}

export interface ProjectResponse {
  projectId: string;
  name: string;
  environment: string;
  owner: string;
  description: string;
  createdAt: string;
  updatedAt: string;
}

export interface SaveProjectRequest {
  projectId: string;
  name: string;
  environment: string;
  owner: string;
  description: string;
}

export interface CatalogPointAddress {
  kind: string;
  value: string;
}

export interface PointSetPointResponse {
  pointId: string;
  semanticId: string;
  address: CatalogPointAddress;
  valueType: string;
  unit?: string | null;
  intervalMs: number;
}

export interface PointSetResponse {
  pointSetId: string;
  projectId: string;
  name: string;
  description: string;
  protocol: string;
  points: PointSetPointResponse[];
  createdAt: string;
  updatedAt: string;
}

export interface SavePointSetRequest {
  pointSetId: string;
  projectId: string;
  name: string;
  description: string;
  protocol: string;
  points: PointSetPointResponse[];
}

export interface ProductResponse {
  productId: string;
  projectId: string;
  name: string;
  productType: string;
  description: string;
  latestVersion?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface SaveProductRequest {
  productId: string;
  projectId: string;
  name: string;
  productType: string;
  description: string;
}

export interface SaveProductVersionRequest {
  version: string;
  pointSetIds: string[];
  deviceModels: unknown[];
  devices: unknown[];
  protocolConnections: unknown[];
  collectionTasks: unknown[];
  algorithms: unknown[];
  dataConfigs: unknown[];
  mqttUplinks: unknown[];
}

export interface ProductVersionResponse {
  productId: string;
  version: string;
  status: 'draft' | 'published' | 'retired';
  pointSetIds: string[];
  deviceModels: unknown[];
  devices: unknown[];
  protocolConnections: unknown[];
  collectionTasks: unknown[];
  algorithms: unknown[];
  dataConfigs: unknown[];
  mqttUplinks: unknown[];
  createdAt: string;
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
  projectId?: string;
  productId?: string;
  desiredProductVersion?: string;
  reportedProductVersion?: string;
  accessToken?: string;
}

export interface CreateEdgeNodeRequest {
  displayName: string;
  productId?: string;
  projectId?: string;
  site: string;
}

export interface BindEdgeProductRequest {
  projectId: string;
  productId: string;
  desiredVersion?: string;
}

export interface EdgeAccessTokenResponse {
  accessToken: string;
  createdAt: string;
  credentialId: string;
  edgeId: string;
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

export interface AuthStatusResponse {
  authenticationEnabled: boolean;
  role: 'viewer' | 'operator' | 'admin';
  subject: string;
}

export interface AgentProviderStatusResponse {
  configured: boolean;
  mode: 'deterministic' | 'openai_compatible';
  model: string;
}

export interface AgentChatRequest {
  message: string;
  projectId?: string | null;
  edgeId?: string | null;
  conversationId?: string | null;
  operatorId?: string;
}

export interface AgentChatResponse {
  message: string;
  mode: 'deterministic' | 'openai_compatible';
  model: string;
  citations: AgentCitationResponse[];
  conversationId?: string;
  conversationTitle?: string;
}

export interface AgentCitationResponse {
  documentId: string;
  title: string;
  sourceUri: string | null;
  excerpt: string;
}

export interface AgentConversationMessageResponse {
  messageId: string;
  role: 'user' | 'assistant';
  content: string;
  citations: AgentCitationResponse[];
  createdAt: string;
}

export interface AgentConversationResponse {
  conversationId: string;
  projectId: string | null;
  edgeId: string | null;
  operatorId: string;
  title: string;
  messages: AgentConversationMessageResponse[];
  createdAt: string;
  updatedAt: string;
}

export interface AgentKnowledgeDocumentResponse {
  documentId: string;
  projectId: string | null;
  title: string;
  sourceUri: string | null;
  content: string;
  tags: string[];
  enabled: boolean;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
}

export interface SaveAgentKnowledgeDocumentRequest {
  projectId?: string | null;
  title: string;
  sourceUri?: string | null;
  content: string;
  tags: string[];
  enabled: boolean;
  actor: string;
}

export type AgentProposalKind =
  | 'config_suggestion'
  | 'point_mapping'
  | 'rollout_plan'
  | 'command_candidate';
export type AgentProposalRisk = 'low' | 'medium' | 'high';
export type AgentProposalStatus = 'pending_review' | 'approved' | 'rejected';

export interface AgentProposalResponse {
  proposalId: string;
  agentId: string;
  kind: AgentProposalKind;
  projectId: string | null;
  edgeId: string | null;
  title: string;
  summary: string;
  payload: unknown;
  risk: AgentProposalRisk;
  status: AgentProposalStatus;
  createdBy: string;
  createdAt: string;
  reviewedBy: string | null;
  reviewedAt: string | null;
  reviewNote: string | null;
}

export interface CreateAgentProposalRequest {
  agentId: string;
  kind: AgentProposalKind;
  projectId?: string | null;
  edgeId?: string | null;
  title: string;
  summary: string;
  payload?: unknown;
  risk: AgentProposalRisk;
  createdBy: string;
}

export interface ReviewAgentProposalRequest {
  reviewer: string;
  note?: string | null;
}

export interface ProtocolConnectionResponse {
  edgeId: string;
  connectionId: string;
  protocolType: string;
  protocol: string;
  endpoint: string;
  serial?: SerialConnectionSettings | null;
  status: string;
  policy: string;
}

export interface SaveProtocolConnectionRequest {
  protocolType: string;
  endpoint: string | null;
  serial?: SerialConnectionSettings | null;
}

export interface CreateProtocolConnectionRequest {
  protocolType: string;
  endpoint: string | null;
  serial?: SerialConnectionSettings | null;
}

export interface SerialConnectionSettings {
  port: string;
  baudRate: number;
  dataBits: number;
  stopBits: number;
  parity: 'none' | 'even' | 'odd';
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

export interface DataConfigCollection {
  periodMs: number;
  timeoutMs: number;
  retryCount: number;
}

export interface DataConfigPoint {
  pointId: string;
  semanticId: string;
  addressKind: string;
  addressValue: string;
  valueType: string;
  unit?: string | null;
  jsonField: string;
}

export interface DataConfigPayload {
  mode: 'object' | 'array';
  timestampField: string;
  includeQuality: boolean;
}

export interface DataConfigPublish {
  sinkId: string;
  topicTemplate: string;
  qos: number;
  payload: DataConfigPayload;
}

export type DataConfigGraphNodeKind = 'point' | 'algorithm' | 'json' | 'mqtt';

export interface DataConfigGraphNode {
  nodeId: string;
  kind: DataConfigGraphNodeKind;
  label: string;
  refId?: string | null;
  params?: Record<string, boolean | number | string | string[]>;
  x: number;
  y: number;
}

export interface DataConfigGraphEdge {
  edgeId: string;
  from: string;
  fromPort?: string | null;
  to: string;
  toPort?: string | null;
}

export interface DataConfigVisualGraph {
  nodes: DataConfigGraphNode[];
  edges: DataConfigGraphEdge[];
}

export interface DataConfigResponse {
  edgeId: string;
  configId: string;
  name: string;
  enabled: boolean;
  deviceId: string;
  protocolConnectionId: string;
  collection: DataConfigCollection;
  points: DataConfigPoint[];
  algorithmIds: string[];
  visualGraph: DataConfigVisualGraph;
  publish: DataConfigPublish;
}

export type SaveDataConfigRequest = Omit<DataConfigResponse, 'edgeId' | 'algorithmIds' | 'visualGraph'> & {
  algorithmIds?: string[];
  visualGraph?: DataConfigVisualGraph;
};

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
      functions: Array<{ function: 'avg' | 'min' | 'max' | 'sum' | 'count' | 'first' | 'last'; output: string }>;
    }
  | { type: 'expression'; output: string; expr: string }
  | { type: 'scale'; source: string; output: string; factor: number; offset: number }
  | { type: 'clamp'; source: string; output: string; min: number; max: number }
  | { type: 'rateOfChange'; source: string; output: string; perMs: number }
  | { type: 'debounce'; source: string; stableMs: number }
  | {
      type: 'conditionalRoute';
      source: string;
      operator: 'Gt' | 'Gte' | 'Lt' | 'Lte' | 'Eq' | 'Ne';
      threshold: number;
      matchedOutput: string;
      unmatchedOutput: string;
    }
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
  username?: string;
  passwordEnv?: string;
  tlsCaPath?: string;
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
