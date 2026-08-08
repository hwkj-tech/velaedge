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
  modbus?: CatalogModbusPointOptions;
}

export interface CatalogModbusPointOptions {
  encoding?: 'u16' | 'i16' | 'u32' | 'i32' | 'u64' | 'i64' | 'f32' | 'f64';
  byteOrder: 'big_endian' | 'little_endian';
  wordOrder: 'high_word_first' | 'low_word_first';
  scale: number;
  offset: number;
  bitIndex?: number;
}

export interface CatalogBacnetPointOptions {
  writePriority: number;
}

export type CatalogOpcUaWriteDataType =
  | 'Boolean'
  | 'SByte'
  | 'Byte'
  | 'Int16'
  | 'UInt16'
  | 'Int32'
  | 'UInt32'
  | 'Int64'
  | 'UInt64'
  | 'Float'
  | 'Double'
  | 'String';

export interface CatalogOpcUaPointOptions {
  writeDataType: CatalogOpcUaWriteDataType;
}

export type CatalogIec104ControlType = 'C_SC_NA_1' | 'C_DC_NA_1' | 'C_SE_NC_1';

export interface CatalogIec104PointOptions {
  controlType: CatalogIec104ControlType;
  selectBeforeOperate: boolean;
}

export type CatalogIec101ControlType = CatalogIec104ControlType;
export type CatalogIec101PointOptions = CatalogIec104PointOptions;

export interface PointSetPointResponse {
  pointId: string;
  semanticId: string;
  address: CatalogPointAddress;
  valueType: string;
  access: 'read_only' | 'read_write' | 'write_only';
  opcUa?: CatalogOpcUaPointOptions;
  iec101?: CatalogIec101PointOptions;
  iec104?: CatalogIec104PointOptions;
  bacnet?: CatalogBacnetPointOptions;
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

export interface Dlt645DataIdentifierTemplateResponse {
  templateId: string;
  name: string;
  semanticId: string;
  dataIdentifier: string;
  valueType: 'Float' | 'Integer' | 'Boolean' | 'Text';
  decimalPlaces: number;
  valueBytes: number;
  unit?: string | null;
}

export interface BacnetObjectTemplateResponse {
  objectType: string;
  name: string;
  rawValue: number;
  writable: boolean;
}

export interface BacnetPropertyTemplateResponse {
  property: string;
  name: string;
  rawValue: number;
}

export interface BacnetIpCatalogResponse {
  objectTypes: BacnetObjectTemplateResponse[];
  properties: BacnetPropertyTemplateResponse[];
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
  commandFlows: CommandFlowConfig[];
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
  commandFlows: CommandFlowConfig[];
  mqttUplinks: unknown[];
  createdAt: string;
}

export type CommandGraphNodeKind =
  | 'mqtt_input'
  | 'json_parse'
  | 'condition'
  | 'safety_gate'
  | 'point_write'
  | 'mqtt_reply';

export interface CommandGraphNode {
  node_id: string;
  kind: CommandGraphNodeKind;
  label: string;
  ref_id?: string;
  params: Record<string, unknown>;
  x: number;
  y: number;
}

export interface CommandGraphEdge {
  edge_id: string;
  from: string;
  from_port?: string;
  to: string;
  to_port?: string;
}

export interface CommandFlowConfig {
  flow_id: string;
  name: string;
  enabled: boolean;
  protocol_connection_id?: string;
  mqtt_connection_id: string;
  subscribe_topic: string;
  qos: number;
  reply_topic_template: string;
  nodes: CommandGraphNode[];
  edges: CommandGraphEdge[];
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
  readWrite?: 'read' | 'read_write' | 'write';
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
  readWrite?: 'read' | 'read_write' | 'write';
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

export interface Iec104ConnectionSettings {
  cp56TimeZoneOffsetMinutes: number;
}

export interface Iec101ConnectionSettings {
  cp56TimeZoneOffsetMinutes: number;
}

export type RuntimeProtocolTransport =
  | 'internal'
  | 'serial'
  | 'tcp'
  | 'udp'
  | 'tcp_udp';

export type RuntimeProtocolMaturity =
  | 'laboratory'
  | 'deployment_candidate'
  | 'production'
  | 'planned';

export interface RuntimeProtocolDescriptor {
  protocolType: string;
  capabilityId: string;
  displayName: string;
  transport: RuntimeProtocolTransport;
  maturity: RuntimeProtocolMaturity;
  telemetryRead: boolean;
  commandWrite: boolean;
  automaticDiscovery: boolean;
}

export interface ProtocolConnectionResponse {
  edgeId: string;
  connectionId: string;
  protocolType: string;
  protocol: string;
  endpoint: string;
  serial?: SerialConnectionSettings | null;
  iec101?: Iec101ConnectionSettings | null;
  iec104?: Iec104ConnectionSettings | null;
  opcUa?: OpcUaConnectionSettings | null;
  bacnetIp?: BacnetIpConnectionSettings | null;
  siemensS7?: SiemensS7ConnectionSettings | null;
  omronFins?: OmronFinsConnectionSettings | null;
  circuitBreaker?: ProtocolCircuitBreakerConfig;
  status: string;
  policy: string;
}

export interface SaveProtocolConnectionRequest {
  protocolType: string;
  endpoint: string | null;
  serial?: SerialConnectionSettings | null;
  iec101?: Iec101ConnectionSettings | null;
  iec104?: Iec104ConnectionSettings | null;
  opcUa?: OpcUaConnectionSettings | null;
  bacnetIp?: BacnetIpConnectionSettings | null;
  siemensS7?: SiemensS7ConnectionSettings | null;
  omronFins?: OmronFinsConnectionSettings | null;
  circuitBreaker?: ProtocolCircuitBreakerConfig;
}

export interface CreateProtocolConnectionRequest {
  protocolType: string;
  endpoint: string | null;
  serial?: SerialConnectionSettings | null;
  iec101?: Iec101ConnectionSettings | null;
  iec104?: Iec104ConnectionSettings | null;
  opcUa?: OpcUaConnectionSettings | null;
  bacnetIp?: BacnetIpConnectionSettings | null;
  siemensS7?: SiemensS7ConnectionSettings | null;
  omronFins?: OmronFinsConnectionSettings | null;
  circuitBreaker?: ProtocolCircuitBreakerConfig;
}

export type OpcUaSecurityPolicy =
  | 'none'
  | 'basic256_sha256'
  | 'aes128_sha256_rsa_oaep'
  | 'aes256_sha256_rsa_pss';

export type OpcUaMessageSecurityMode = 'none' | 'sign' | 'sign_and_encrypt';
export type OpcUaAuthMode = 'anonymous' | 'username' | 'x509';

export interface OpcUaConnectionSettings {
  securityPolicy: OpcUaSecurityPolicy;
  messageSecurityMode: OpcUaMessageSecurityMode;
  authMode: OpcUaAuthMode;
  username?: string | null;
  passwordEnv?: string | null;
  userCertificatePath?: string | null;
  userPrivateKeyPath?: string | null;
  pkiDir: string;
  trustServerCerts: boolean;
  verifyServerCerts: boolean;
  connectTimeoutMs: number;
  requestTimeoutMs: number;
  sessionTimeoutMs: number;
  sessionRetryLimit: number;
}

export interface BacnetIpConnectionSettings {
  bindAddress: string;
  localPort: number;
  broadcastAddress: string;
  apduTimeoutMs: number;
  apduRetries: number;
  discoveryTimeoutMs: number;
  maxApduLength: 50 | 128 | 206 | 480 | 1024 | 1476;
  foreignDevice?: BacnetForeignDeviceSettings | null;
  cov?: BacnetCovSettings | null;
}

export interface BacnetForeignDeviceSettings {
  bbmdAddress: string;
  ttlSeconds: number;
}

export interface BacnetCovSettings {
  lifetimeSeconds: number;
  confirmedNotifications: boolean;
  fallbackPollIntervalMs: number;
}

export interface SiemensS7ConnectionSettings {
  rack: number;
  slot: number;
  pduSize: 240 | 480 | 960;
  connectTimeoutMs: number;
  requestTimeoutMs: number;
}

export interface OmronFinsConnectionSettings {
  transport: 'udp' | 'tcp';
  sourceNetwork: number;
  sourceNode: number;
  sourceUnit: number;
  destinationNetwork: number;
  destinationNode: number;
  destinationUnit: number;
  timeoutMs: number;
  wordOrder: 'high_word_first' | 'low_word_first';
}

export interface ProtocolCircuitBreakerConfig {
  enabled: boolean;
  failureThreshold: number;
  openDurationMs: number;
  halfOpenSuccessThreshold: number;
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
      type: 'durationCondition';
      source: string;
      operator: 'Gt' | 'Gte' | 'Lt' | 'Lte' | 'Eq' | 'Ne';
      threshold: number;
      durationMs: number;
      output: string;
    }
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
  collection_attempt_count?: number;
  collection_success_count?: number;
  write_attempt_count?: number;
  write_success_count?: number;
  circuit_state?: 'Closed' | 'Open' | 'HalfOpen';
  consecutive_failure_count?: number;
  circuit_open_count?: number;
  circuit_rejected_count?: number;
  last_quality_code?:
    | 'good'
    | 'uncertain_protocol'
    | 'uncertain_last_known'
    | 'uncertain_out_of_range'
    | 'uncertain_substituted'
    | 'uncertain_overflow'
    | 'bad_communication'
    | 'bad_timeout'
    | 'bad_protocol'
    | 'bad_decode'
    | 'bad_configuration'
    | 'bad_out_of_service';
  good_value_count?: number;
  uncertain_value_count?: number;
  bad_value_count?: number;
  subscription_count?: number;
  notification_count?: number;
  subscription_error_count?: number;
  fallback_poll_count?: number;
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

export interface MqttSinkRuntimeMetrics {
  sink_id: string;
  broker: string;
  client_id: string;
  connected: boolean;
  publish_success_count: number;
  publish_failure_count: number;
  published_bytes: number;
  average_ack_latency_ms: number;
  last_ack_latency_ms?: number | null;
  last_publish_at?: string | null;
  last_topic?: string | null;
  last_error?: string | null;
}

export interface MqttRuntimeMetrics {
  configured_sink_count: number;
  connected_sink_count: number;
  connection_generation: number;
  publish_success_count: number;
  publish_failure_count: number;
  published_bytes: number;
  sinks: MqttSinkRuntimeMetrics[];
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
  mqtt?: MqttRuntimeMetrics;
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
  protocolVersion?: '3.1.1' | '5.0';
  keepAliveSeconds?: number;
  cleanSession?: boolean;
  cleanStart?: boolean;
  sessionExpiryIntervalSeconds?: number;
  receiveMaximum?: number;
  maximumPacketSizeBytes?: number;
  topicAliasMaximum?: number;
  requestResponseInformation?: boolean;
  requestProblemInformation?: boolean;
  userProperties?: MqttUserProperty[];
  lastWill?: MqttLastWill;
  username?: string;
  passwordEnv?: string;
  tlsCaPath?: string;
  topicTemplate: string;
  qos: number;
  batchSize: number;
  flushIntervalMs: number;
}

export interface MqttUserProperty {
  key: string;
  value: string;
}

export interface MqttLastWill {
  topic: string;
  payload: string;
  qos: number;
  retain: boolean;
  delayIntervalSeconds?: number;
  payloadFormatUtf8?: boolean;
  messageExpiryIntervalSeconds?: number;
  contentType?: string;
  responseTopic?: string;
  correlationData?: string;
  userProperties?: MqttUserProperty[];
}

export type SaveMqttUplinkRequest = MqttUplinkResponse;

export interface RunDiscoveryRequest {
  connectionId: string;
  addressRange?: string;
  rootNodeId?: string;
  maxDepth?: number;
  includeStandardNamespace?: boolean;
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
