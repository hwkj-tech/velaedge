import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  bindEdgeProduct,
  createAgentProposal,
  createAlgorithmDraft,
  createCollectionTaskDraft,
  createEdgeDataConfig,
  createEdgeNode,
  createProduct,
  createProductVersion,
  createProject,
  createPointMappingDraft,
  fetchAlgorithms,
  fetchAgentProposals,
  fetchAgentProviderStatus,
  fetchAuditRecords,
  fetchAuthStatus,
  fetchCollectionTasks,
  fetchDeviceModels,
  fetchDlt645DataIdentifiers,
  fetchEdgeAlgorithms,
  fetchEdgeCollectionTasks,
  fetchEdgeDataConfigs,
  fetchEdgePointMappings,
  fetchEdgeProtocolConnections,
  fetchEdgeNodes,
  fetchPointMappings,
  fetchPointSets,
  fetchProducts,
  fetchProductVersions,
  fetchProjects,
  fetchProtocolCatalog,
  fetchProtocolConnections,
  fetchReleaseList,
  fetchRuntimeStatus,
  fetchSummary,
  fetchMqttUplink,
  fetchDiscoverySuggestions,
  generateEdgeAccessToken,
  generateAgentSuggestions,
  publishLatestRelease,
  reviewAgentProposal,
  runDiscovery,
  runAgentSafetyCheck,
  runConfigValidation,
  runReleaseDiff,
  createEdgeProtocolConnection,
  deleteEdgeNode,
  deleteEdgeDataConfig,
  deleteProduct,
  deleteProject,
  saveEdgeAlgorithm,
  saveEdgeCollectionTask,
  saveEdgeDataConfig,
  saveEdgePointMapping,
  saveEdgeProtocolConnection,
  saveMqttUplink,
  saveProduct,
  saveProductVersion,
  saveProject,
  sendAgentChat,
  setApiToken,
} from './api/client';
import type {
  AlgorithmResponse,
  AuditRecordResponse,
  CollectionTaskResponse,
  DataConfigResponse,
  DeviceModelResponse,
  EdgeNodeResponse,
  PointMappingResponse,
  PointSetResponse,
  ProductResponse,
  ProductVersionResponse,
  ProjectResponse,
  ProtocolConnectionResponse,
  ReleaseListResponse,
  RuntimeStatusResponse,
  SaveAlgorithmRequest,
  SaveCollectionTaskRequest,
  SaveDataConfigRequest,
  SaveProtocolConnectionRequest,
} from './api/types';
import {
  ConsoleApp as App,
  buildProductPlannerGraph,
  buildProductVersionRequest,
  EDGE_CONFIG_TEMPLATES,
  hydrateProductTemplate,
  materializeProductRuntime,
} from './App';

vi.mock('./api/client', () => ({
  bindEdgeProduct: vi.fn(),
  createAgentProposal: vi.fn(),
  createAlgorithmDraft: vi.fn(),
  createCollectionTaskDraft: vi.fn(),
  createEdgeDataConfig: vi.fn(),
  createEdgeNode: vi.fn(),
  createProduct: vi.fn(),
  createProductVersion: vi.fn(),
  createProject: vi.fn(),
  createDeviceModelDraft: vi.fn(),
  createPointMappingDraft: vi.fn(),
  fetchAlgorithms: vi.fn(),
  fetchAgentProposals: vi.fn(),
  fetchAgentProviderStatus: vi.fn(),
  fetchAuditRecords: vi.fn(),
  fetchAuthStatus: vi.fn(),
  fetchCollectionTasks: vi.fn(),
  fetchDeviceModels: vi.fn(),
  fetchDlt645DataIdentifiers: vi.fn(),
  fetchEdgeAlgorithms: vi.fn(),
  fetchEdgeCollectionTasks: vi.fn(),
  fetchEdgeDataConfigs: vi.fn(),
  fetchEdgePointMappings: vi.fn(),
  fetchEdgeProtocolConnections: vi.fn(),
  fetchEdgeNodes: vi.fn(),
  fetchPointMappings: vi.fn(),
  fetchPointSets: vi.fn(),
  fetchProducts: vi.fn(),
  fetchProductVersions: vi.fn(),
  fetchProjects: vi.fn(),
  fetchProtocolCatalog: vi.fn(),
  fetchProtocolConnections: vi.fn(),
  fetchReleaseList: vi.fn(),
  fetchRuntimeStatus: vi.fn(),
  fetchSummary: vi.fn(),
  fetchMqttUplink: vi.fn(),
  fetchDiscoverySuggestions: vi.fn(),
  generateEdgeAccessToken: vi.fn(),
  generateAgentSuggestions: vi.fn(),
  publishLatestRelease: vi.fn(),
  reviewAgentProposal: vi.fn(),
  runDiscovery: vi.fn(),
  runAgentSafetyCheck: vi.fn(),
  runConfigValidation: vi.fn(),
  runReleaseDiff: vi.fn(),
  createEdgeProtocolConnection: vi.fn(),
  deleteEdgeNode: vi.fn(),
  deleteEdgeDataConfig: vi.fn(),
  deleteProduct: vi.fn(),
  deleteProject: vi.fn(),
  saveEdgeAlgorithm: vi.fn(),
  saveEdgeCollectionTask: vi.fn(),
  saveEdgeDataConfig: vi.fn(),
  saveEdgePointMapping: vi.fn(),
  saveEdgeProtocolConnection: vi.fn(),
  saveMqttUplink: vi.fn(),
  saveProduct: vi.fn(),
  saveProductVersion: vi.fn(),
  saveProject: vi.fn(),
  sendAgentChat: vi.fn(),
  setApiToken: vi.fn(),
}));

const basePoint: PointMappingResponse = {
  edgeId: 'edge-dev',
  pointId: 'pressure',
  pointName: '泵出口压力',
  deviceId: 'pump-1',
  deviceModel: 'pump@v1',
  semanticTelemetry: 'pump.pressure',
  protocol: 'Modbus TCP',
  connection: 'modbus-line-a',
  address: 'holding_register:40001',
  valueType: 'float32',
  readWrite: 'read',
  unit: 'MPa',
  scale: '0.1',
  interval: '1000ms',
  range: '0-20',
  qualityRule: 'timeout->bad',
  status: '启用',
};

const initialReleaseList: ReleaseListResponse = {
  draftVersion: '2026.06.26-001',
  validationStatus: '已通过',
  changeSummary: '云端配置包已生成',
  rolloutPolicy: '单边端发布',
  applyResults: [
    {
      edgeId: 'edge-dev',
      desiredVersion: '2026.06.26-001',
      reportedVersion: '2026.06.26-001',
      result: '已应用',
      heartbeat: '18 秒前',
    },
  ],
};

const updatedReleaseList: ReleaseListResponse = {
  ...initialReleaseList,
  draftVersion: '2026.06.26-002',
  applyResults: [
    {
      edgeId: 'edge-dev',
      desiredVersion: '2026.06.26-002',
      reportedVersion: '2026.06.26-002',
      result: '已应用',
      heartbeat: '18 秒前',
    },
  ],
};

const mqttUplink = {
  sinkId: 'velamq-main',
  broker: 'mqtts://velamq.local:8883',
  clientId: 'edge-dev-runtime-dev',
  topicTemplate: 'edge/{edge_id}/device/{device_id}/telemetry',
  qos: 1,
  batchSize: 100,
  flushIntervalMs: 1000,
};

const discoverySuggestions = [
  {
    pointId: 'meter_voltage_a',
    deviceId: 'meter-1',
    semanticId: 'electric.voltage_a',
    protocolConnectionId: 'meter-rs485-bus-1',
    address: 'holding_register:40001',
    valueType: 'float32',
    unit: 'V',
    confidence: 0.82,
    evidence: '数值范围和波动特征符合 A 相电压',
  },
];

const edgeNodes: EdgeNodeResponse[] = [
  {
    edgeId: 'edge-dev',
    displayName: '研发实验室边端',
    site: '研发/实验室',
    runtimeId: 'runtime-dev',
    status: '健康',
    resources: '18.5% / 42% / 61%',
    heartbeat: '8 秒前',
    capabilities: ['protocol:modbus-tcp'],
    projectId: 'demo-plant',
    productId: 'pump-collection-uplink',
    desiredProductVersion: 'v1.4.3',
    reportedProductVersion: '2026.06.26-001',
  },
];

const maintenanceEdgeNodes: EdgeNodeResponse[] = [
  {
    ...edgeNodes[0],
    status: '维护中',
    capabilities: [...edgeNodes[0].capabilities, 'mode:maintenance'],
  },
];

const deviceModels: DeviceModelResponse[] = [
  {
    deviceType: 'pump',
    version: 'v1',
    commandCount: 1,
    eventCount: 1,
    telemetry: [
      {
        telemetryId: 'pressure',
        name: 'pressure',
        valueType: 'float32',
        unit: 'MPa',
        range: '0-20',
        description: '泵出口压力',
      },
    ],
  },
];

const protocolConnections: ProtocolConnectionResponse[] = [
  {
    edgeId: 'edge-dev',
    connectionId: 'modbus-line-a',
    protocol: 'Modbus TCP',
    protocolType: 'ModbusTcp',
    endpoint: '10.12.0.20:502',
    status: '启用',
    policy: '1000ms timeout / 3 retry',
  },
];

const createdProtocolConnection: ProtocolConnectionResponse = {
  edgeId: 'edge-dev',
  connectionId: 'connection-draft-2',
  protocol: 'Modbus TCP',
  protocolType: 'ModbusTcp',
  endpoint: 'runtime://pending',
  status: '启用',
  policy: '1000ms timeout / 3 retry',
};

const collectionTasks: CollectionTaskResponse[] = [
  {
    edgeId: 'edge-dev',
    taskId: 'pump-main',
    deviceId: 'pump-1',
    pointIds: ['pressure', 'running'],
    pointList: 'pressure, running',
    intervalMs: 1000,
    interval: '1000ms',
    enabled: true,
    status: '启用',
  },
];

const dataConfigs: DataConfigResponse[] = [
  {
    edgeId: 'edge-dev',
    configId: 'pump_status',
    name: '泵状态上报',
    enabled: true,
    deviceId: 'pump-1',
    protocolConnectionId: 'modbus-line-a',
    collection: { periodMs: 1000, retryCount: 2, timeoutMs: 800 },
    algorithmIds: ['pump-anomaly-v1'],
    points: [
      {
        addressKind: 'holding_register',
        addressValue: '40001',
        jsonField: 'pressure',
        pointId: 'pressure',
        semanticId: 'pump.pressure',
        unit: 'MPa',
        valueType: 'float32',
      },
    ],
    publish: {
      payload: { includeQuality: true, mode: 'object', timestampField: 'ts' },
      qos: 1,
      sinkId: 'velamq-main',
      topicTemplate: 'factory/{edge_id}/{device_id}/status',
    },
    visualGraph: {
      edges: [
        { edgeId: 'point-pressure-to-json', from: 'point-pressure', to: 'json-payload' },
        { edgeId: 'json-to-mqtt', from: 'json-payload', to: 'mqtt-output' },
      ],
      nodes: [
        { kind: 'point', label: 'pressure', nodeId: 'point-pressure', refId: 'pressure', x: 56, y: 56 },
        { kind: 'json', label: 'JSON Payload', nodeId: 'json-payload', refId: null, x: 520, y: 96 },
        { kind: 'mqtt', label: 'MQTT Topic', nodeId: 'mqtt-output', refId: 'factory/{edge_id}/{device_id}/status', x: 720, y: 96 },
      ],
    },
  },
];

const createdPoint: PointMappingResponse = {
  ...basePoint,
  address: 'simulated:point-draft-2',
  pointId: 'point-draft-2',
  pointName: 'point-draft-2',
  semanticTelemetry: 'pump.point-draft-2',
};

const createdCollectionTask: CollectionTaskResponse = {
  ...collectionTasks[0],
  taskId: 'task-draft-2',
};

const changeReportDsl = {
  inputs: [{ alias: 'p', pointId: 'pressure' }],
  trigger: { type: 'onSample' as const },
  steps: [{ type: 'changeFilter' as const, source: 'p', threshold: 0.2 }],
  outputs: [{ name: 'reported', pointId: 'pressure.reported' }],
  report: { mode: 'OnChange' as const, sink: 'velamq-main' },
};

const algorithms: AlgorithmResponse[] = [
  {
    edgeId: 'edge-dev',
    algorithmId: 'pump-anomaly-v1',
    version: '1.0.0',
    algorithmKind: 'ChangeReport',
    dsl: changeReportDsl,
    runtime: 'Rule',
    kind: '变化上报',
    inputIds: ['pressure'],
    outputIds: ['pressure.reported'],
    inputs: 'pressure',
    outputs: 'pressure.reported',
    execution: '边端本地执行',
    validation: '已通过',
  },
];

const createdAlgorithm: AlgorithmResponse = {
  ...algorithms[0],
  algorithmId: 'algorithm-draft-2',
  algorithmKind: 'ThresholdRule',
  inputIds: ['pressure'],
  inputs: 'pressure',
  outputIds: ['thermal.alert'],
  outputs: 'thermal.alert',
  runtime: 'Rule',
};

const auditRecords: AuditRecordResponse[] = [
  {
    createdAt: '2026-06-26T10:00:00Z',
    time: '10:00:00',
    actor: 'system',
    action: 'create_release',
    target: 'release-1',
    result: '成功',
  },
];

const runtimeStatus: RuntimeStatusResponse = {
  healthyEdgeCount: 1,
  degradedEdgeCount: 0,
  criticalEdgeCount: 0,
  averageCollectionLatencyMs: 24,
  edges: [
    {
      edge_id: 'edge-dev',
      runtime_id: 'runtime-dev',
      config_version: '2026.06.26-001',
      timestamp: '2026-06-26T10:00:00Z',
      health: 'Healthy',
      system: {
        cpu_percent: 18.5,
        memory_percent: 42,
        disk_percent: 61,
        process_uptime_seconds: 3600,
      },
      collection: {
        active_task_count: 1,
        success_rate: 0.995,
        average_latency_ms: 24,
        bad_point_count: 0,
      },
      protocols: [
        {
          connection_id: 'modbus-line-a',
          protocol: 'Modbus TCP',
          connected: true,
          latency_ms: 12,
          timeout_count: 0,
          error_count: 0,
          reconnect_count: 0,
        },
      ],
      local_store: {
        backend: 'jsonl',
        buffered_records: 0,
        oldest_buffer_age_seconds: 0,
        disk_usage_percent: 35,
      },
      algorithms: [],
      cloud_sync: {
        connected: true,
        last_sync_seconds_ago: 8,
        pending_uploads: 0,
        desired_version: '2026.06.26-001',
        reported_version: '2026.06.26-001',
      },
    },
  ],
  events: [],
};

const catalogProjects: ProjectResponse[] = [
  {
    createdAt: '2026-06-26T00:00:00Z',
    description: '研发实验室项目',
    environment: 'staging',
    name: 'demo-plant',
    owner: 'platform-team',
    projectId: 'demo-plant',
    updatedAt: '2026-06-26T00:00:00Z',
  },
];

const catalogPointSets: PointSetResponse[] = [
  {
    createdAt: '2026-06-26T00:00:00Z',
    description: '泵站标准点位',
    name: '泵站标准点位',
    pointSetId: 'pump-standard-points',
    points: [
      {
        access: 'read_only',
        address: { kind: 'holding_register', value: '40011' },
        intervalMs: 1000,
        pointId: 'catalog_temperature',
        semanticId: 'environment.temperature',
        unit: 'C',
        valueType: 'float32',
      },
    ],
    projectId: 'demo-plant',
    protocol: 'ModbusRtu',
    updatedAt: '2026-06-26T00:00:00Z',
  },
];

const catalogProducts: ProductResponse[] = EDGE_CONFIG_TEMPLATES.map((template) => ({
  createdAt: '2026-06-26T00:00:00Z',
  description: template.description,
  latestVersion: template.version,
  name: template.name,
  productId: template.templateId,
  productType: template.productType,
  projectId: template.projectId,
  updatedAt: '2026-06-26T00:00:00Z',
}));

describe('App cloud console write actions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(fetchAuthStatus).mockResolvedValue({
      authenticationEnabled: false,
      role: 'admin',
      subject: 'local-development',
    });
    vi.mocked(fetchSummary).mockResolvedValue({
      edge_count: 1,
      pending_release_count: 0,
    });
    vi.mocked(fetchEdgeNodes).mockResolvedValue(edgeNodes);
    vi.mocked(fetchDeviceModels).mockResolvedValue(deviceModels);
    vi.mocked(fetchDlt645DataIdentifiers).mockResolvedValue([]);
    vi.mocked(fetchProtocolConnections).mockResolvedValue(protocolConnections);
    vi.mocked(fetchEdgeProtocolConnections).mockResolvedValue(protocolConnections);
    vi.mocked(fetchPointMappings).mockResolvedValue([basePoint]);
    vi.mocked(fetchPointSets).mockResolvedValue(catalogPointSets);
    vi.mocked(fetchProducts).mockResolvedValue(catalogProducts);
    vi.mocked(fetchProductVersions).mockImplementation(async (productId) => {
      const product = catalogProducts.find((candidate) => candidate.productId === productId);
      return product
        ? [
            {
              algorithms: [],
              collectionTasks: [],
              commandFlows: [],
              createdAt: '2026-06-26T00:00:00Z',
              dataConfigs: [],
              deviceModels: [],
              devices: [],
              mqttUplinks: [],
              pointSetIds: [],
              productId,
              protocolConnections: [],
              status: 'published',
              version: product.latestVersion ?? 'v1.0.0',
            },
          ]
        : [];
    });
    vi.mocked(fetchProjects).mockResolvedValue(catalogProjects);
    vi.mocked(fetchProtocolCatalog).mockResolvedValue([
      {
        automaticDiscovery: false,
        capabilityId: 'modbus-tcp',
        commandWrite: true,
        displayName: 'Modbus TCP',
        maturity: 'deployment_candidate',
        protocolType: 'ModbusTcp',
        telemetryRead: true,
        transport: 'tcp',
      },
      {
        automaticDiscovery: true,
        capabilityId: 'modbus-rtu',
        commandWrite: true,
        displayName: 'Modbus RTU',
        maturity: 'deployment_candidate',
        protocolType: 'ModbusRtu',
        telemetryRead: true,
        transport: 'serial',
      },
    ]);
    vi.mocked(fetchEdgePointMappings).mockResolvedValue([basePoint]);
    vi.mocked(fetchCollectionTasks).mockResolvedValue(collectionTasks);
    vi.mocked(fetchEdgeCollectionTasks).mockResolvedValue(collectionTasks);
    vi.mocked(fetchEdgeDataConfigs).mockResolvedValue(dataConfigs);
    vi.mocked(fetchAlgorithms).mockResolvedValue(algorithms);
    vi.mocked(fetchEdgeAlgorithms).mockResolvedValue(algorithms);
    vi.mocked(fetchReleaseList).mockResolvedValue(initialReleaseList);
    vi.mocked(fetchRuntimeStatus).mockResolvedValue(runtimeStatus);
    vi.mocked(fetchAuditRecords).mockResolvedValue(auditRecords);
    vi.mocked(fetchMqttUplink).mockResolvedValue(mqttUplink);
    vi.mocked(fetchDiscoverySuggestions).mockResolvedValue(discoverySuggestions);
    vi.mocked(fetchAgentProposals).mockResolvedValue([]);
    vi.mocked(fetchAgentProviderStatus).mockResolvedValue({
      configured: false,
      mode: 'deterministic',
      model: 'edgeops-local-analysis',
    });
    vi.mocked(sendAgentChat).mockResolvedValue({
      citations: [],
      message: '当前边端健康，建议先校验配置差异。',
      mode: 'deterministic',
      model: 'edgeops-local-analysis',
    });
    vi.mocked(createEdgeNode).mockResolvedValue({
      edgeId: 'edge-draft-1',
      displayName: '新边端',
      site: '待分配',
      runtimeId: '-',
      status: '未上报',
      resources: '-',
      heartbeat: '-',
      capabilities: ['product:pump-collection-uplink'],
      projectId: 'demo-plant',
      productId: 'pump-collection-uplink',
      desiredProductVersion: 'v1.4.3',
      accessToken: 'edge_created_secret',
    });
    vi.mocked(generateEdgeAccessToken).mockResolvedValue({
      accessToken: 'edge_regenerated_secret',
      createdAt: '2026-06-26T00:00:00Z',
      credentialId: 'credential-1',
      edgeId: 'edge-dev',
    });
    vi.mocked(bindEdgeProduct).mockResolvedValue(edgeNodes[0]);
    vi.mocked(deleteEdgeNode).mockResolvedValue(undefined);
    vi.mocked(deleteProject).mockResolvedValue(undefined);
    vi.mocked(deleteProduct).mockResolvedValue(undefined);
    vi.mocked(createProduct).mockImplementation(async (request) => ({
      ...request,
      createdAt: '2026-06-26T00:00:00Z',
      latestVersion: null,
      updatedAt: '2026-06-26T00:00:00Z',
    }));
    vi.mocked(saveProduct).mockImplementation(async (_productId, request) => ({
      ...request,
      createdAt: '2026-06-26T00:00:00Z',
      latestVersion: 'v1.4.3',
      updatedAt: '2026-06-26T00:00:00Z',
    }));
    vi.mocked(createProductVersion).mockImplementation(async (productId, request) => ({
      ...request,
      createdAt: '2026-06-26T00:00:00Z',
      productId,
      status: 'draft',
    }));
    vi.mocked(saveProductVersion).mockImplementation(
      async (productId, _version, request) => ({
        ...request,
        createdAt: '2026-06-26T00:00:00Z',
        productId,
        status: 'draft',
      }),
    );
    vi.mocked(createProject).mockImplementation(async (request) => ({
      ...request,
      createdAt: '2026-06-26T00:00:00Z',
      updatedAt: '2026-06-26T00:00:00Z',
    }));
    vi.mocked(saveProject).mockImplementation(async (_projectId, request) => ({
      ...request,
      createdAt: '2026-06-26T00:00:00Z',
      updatedAt: '2026-06-26T00:00:00Z',
    }));
    vi.mocked(createAlgorithmDraft).mockResolvedValue(createdAlgorithm);
    vi.mocked(createCollectionTaskDraft).mockResolvedValue(createdCollectionTask);
    vi.mocked(createPointMappingDraft).mockResolvedValue(createdPoint);
    vi.mocked(generateAgentSuggestions).mockResolvedValue({
      action: 'agent_generate_suggestions',
      details: ['建议 3 条'],
      message: 'Agent 建议已生成',
      status: '待确认',
      suggestions: [
        {
          detail: '根据 pump@v1 模型发现缺少 flow_rate 映射',
          state: '生成候选配置',
          title: '点位补全',
        },
      ],
    });
    vi.mocked(runAgentSafetyCheck).mockResolvedValue({
      action: 'agent_safety_check',
      details: ['危险命令需要人工确认'],
      message: '安全策略检查已完成',
      status: '已通过',
      suggestions: [],
    });
    vi.mocked(runConfigValidation).mockResolvedValue({
      action: 'validate_config',
      details: ['协议连接 1 个'],
      message: '配置校验已完成',
      status: '已通过',
    });
    vi.mocked(runReleaseDiff).mockResolvedValue({
      action: 'release_diff',
      details: ['新增 1 个配置项'],
      message: '配置差异摘要已生成',
      status: '已生成',
    });
    vi.mocked(saveEdgePointMapping).mockResolvedValue({
      ...basePoint,
      address: 'holding_register:40002',
      interval: '2000ms',
    });
    vi.mocked(saveEdgeCollectionTask).mockResolvedValue({
      ...collectionTasks[0],
      enabled: false,
      interval: '2500ms',
      intervalMs: 2500,
      pointIds: ['pressure'],
      pointList: 'pressure',
      status: '暂停',
    });
    vi.mocked(createEdgeDataConfig).mockResolvedValue(dataConfigs[0]);
    vi.mocked(saveEdgeDataConfig).mockResolvedValue({
      ...dataConfigs[0],
      name: '泵状态',
    });
    vi.mocked(deleteEdgeDataConfig).mockResolvedValue(undefined);
    vi.mocked(saveEdgeProtocolConnection).mockResolvedValue({
      ...protocolConnections[0],
      endpoint: 'opc.tcp://10.12.0.80:4840',
      protocol: 'OPC UA',
      protocolType: 'OpcUa',
    });
    vi.mocked(createEdgeProtocolConnection).mockResolvedValue(createdProtocolConnection);
    vi.mocked(saveEdgeAlgorithm).mockResolvedValue({
      ...algorithms[0],
      algorithmKind: 'WindowAggregate',
      dsl: {
        inputs: [{ alias: 'p', pointId: 'pressure' }],
        trigger: { type: 'window', everyMs: 60000 },
        steps: [
          {
            type: 'windowAggregate',
            source: 'p',
            functions: [{ function: 'avg', output: 'avg_1m' }],
          },
        ],
        outputs: [{ name: 'avg_1m', pointId: 'pressure.avg_1m' }],
        report: { mode: 'WindowResult', sink: 'velamq-main' },
      },
      inputIds: ['pressure'],
      inputs: 'pressure',
      kind: '窗口聚合',
      outputIds: ['pressure.avg_1m'],
      outputs: 'pressure.avg_1m',
      runtime: 'Rule',
      version: '1.1.0',
    });
    vi.mocked(publishLatestRelease).mockResolvedValue(updatedReleaseList);
    vi.mocked(saveMqttUplink).mockResolvedValue(mqttUplink);
    vi.mocked(runDiscovery).mockResolvedValue({
      jobId: 'discovery-edge-dev-1',
      protocolConnectionId: 'meter-rs485-bus-1',
      discoveredPoints: [],
      suggestions: discoverySuggestions,
    });
  });

  it('loads a production empty database without querying an imaginary edge', async () => {
    vi.mocked(fetchSummary).mockResolvedValue({
      edge_count: 0,
      pending_release_count: 0,
    });
    vi.mocked(fetchEdgeNodes).mockResolvedValue([]);
    vi.mocked(fetchDeviceModels).mockResolvedValue([]);
    vi.mocked(fetchPointSets).mockResolvedValue([]);
    vi.mocked(fetchProducts).mockResolvedValue([]);
    vi.mocked(fetchProjects).mockResolvedValue([]);
    vi.mocked(fetchReleaseList).mockResolvedValue({
      applyResults: [],
      changeSummary: '暂无配置',
      draftVersion: '-',
      rolloutPolicy: '未配置',
      validationStatus: '未校验',
    });
    vi.mocked(fetchRuntimeStatus).mockResolvedValue({
      averageCollectionLatencyMs: 0,
      criticalEdgeCount: 0,
      degradedEdgeCount: 0,
      edges: [],
      events: [],
      healthyEdgeCount: 0,
    });
    vi.mocked(fetchAuditRecords).mockResolvedValue([]);

    render(<App />);

    expect(await screen.findByText('项目: 暂无项目')).toBeInTheDocument();
    expect(fetchEdgeProtocolConnections).not.toHaveBeenCalled();
    expect(fetchEdgePointMappings).not.toHaveBeenCalled();
    expect(fetchEdgeCollectionTasks).not.toHaveBeenCalled();
    expect(fetchEdgeDataConfigs).not.toHaveBeenCalled();
    expect(fetchEdgeAlgorithms).not.toHaveBeenCalled();
    expect(fetchMqttUplink).not.toHaveBeenCalled();
    expect(fetchDiscoverySuggestions).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: /项目管理/ }));
    expect(await screen.findByText(/尚未创建项目/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '新建项目' }));
    await waitFor(() => expect(createProject).toHaveBeenCalledTimes(1));
    expect(screen.getByRole('dialog', { name: '项目详情' })).toBeInTheDocument();
  });

  it('materializes the visual product graph without collapsing compute or MQTT outputs', () => {
    const template = JSON.parse(
      JSON.stringify(EDGE_CONFIG_TEMPLATES[1]),
    ) as (typeof EDGE_CONFIG_TEMPLATES)[number];
    template.dataConfig.visualGraph = {
      nodes: [
        { kind: 'point', label: 'pressure', nodeId: 'point-pump_pressure', refId: 'pump_pressure', x: 50, y: 80 },
        { kind: 'point', label: 'running', nodeId: 'point-pump_running', refId: 'pump_running', x: 50, y: 180 },
        { kind: 'algorithm', label: '多点合并', nodeId: 'algorithm-merge_points', refId: 'merge_points', x: 320, y: 90 },
        { kind: 'algorithm', label: '变化上报', nodeId: 'algorithm-change_report', refId: 'change_report', x: 320, y: 220 },
        { kind: 'mqtt', label: '状态主题', nodeId: 'mqtt-status', refId: 'factory/{edge_id}/status', x: 650, y: 90 },
        { kind: 'mqtt', label: '变化主题', nodeId: 'mqtt-change', refId: 'factory/{edge_id}/change', x: 650, y: 220 },
      ],
      edges: [
        { edgeId: 'pressure-merge', from: 'point-pump_pressure', to: 'algorithm-merge_points' },
        { edgeId: 'running-merge', from: 'point-pump_running', to: 'algorithm-merge_points' },
        { edgeId: 'merge-status', from: 'algorithm-merge_points', to: 'mqtt-status' },
        { edgeId: 'pressure-change', from: 'point-pump_pressure', to: 'algorithm-change_report' },
        { edgeId: 'change-output', from: 'algorithm-change_report', to: 'mqtt-change' },
      ],
    };

    const materialized = materializeProductRuntime(template, 'modbus-line-a');

    expect(materialized.algorithms).toHaveLength(1);
    expect(materialized.algorithms[0]).toMatchObject({
      algorithmId: 'pump_status_telemetry__change_report',
      algorithmKind: 'ChangeReport',
    });
    expect(materialized.dataConfig.algorithmIds).toEqual([
      'pump_status_telemetry__change_report',
    ]);
    expect(materialized.dataConfig.visualGraph?.nodes.filter((node) => node.kind === 'mqtt'))
      .toHaveLength(2);
    expect(materialized.dataConfig.visualGraph?.edges).toHaveLength(5);
    expect(
      materialized.dataConfig.visualGraph?.nodes.find(
        (node) => node.nodeId === 'algorithm-change_report',
      )?.refId,
    ).toBe('pump_status_telemetry__change_report');
  });

  it('materializes repeated compute types as independent runtime algorithms', () => {
    const template = JSON.parse(
      JSON.stringify(EDGE_CONFIG_TEMPLATES[1]),
    ) as (typeof EDGE_CONFIG_TEMPLATES)[number];
    template.dataConfig.visualGraph = {
      nodes: [
        { kind: 'point', label: 'pressure', nodeId: 'point-pump_pressure', refId: 'pump_pressure', x: 50, y: 80 },
        { kind: 'point', label: 'running', nodeId: 'point-pump_running', refId: 'pump_running', x: 50, y: 180 },
        { kind: 'algorithm', label: '压力窗口', nodeId: 'algorithm-window_aggregate', refId: 'window_aggregate', x: 320, y: 80 },
        { kind: 'algorithm', label: '状态窗口', nodeId: 'algorithm-window_aggregate-2', refId: 'window_aggregate', x: 320, y: 180 },
        { kind: 'mqtt', label: '压力主题', nodeId: 'mqtt-pressure', refId: 'factory/{edge_id}/pressure', x: 650, y: 80 },
        { kind: 'mqtt', label: '状态主题', nodeId: 'mqtt-status', refId: 'factory/{edge_id}/status', x: 650, y: 180 },
      ],
      edges: [
        { edgeId: 'pressure-window', from: 'point-pump_pressure', to: 'algorithm-window_aggregate' },
        { edgeId: 'window-pressure', from: 'algorithm-window_aggregate', to: 'mqtt-pressure' },
        { edgeId: 'running-window', from: 'point-pump_running', to: 'algorithm-window_aggregate-2' },
        { edgeId: 'window-status', from: 'algorithm-window_aggregate-2', to: 'mqtt-status' },
      ],
    };

    const materialized = materializeProductRuntime(template, 'modbus-line-a');

    expect(materialized.algorithms.map((algorithm) => algorithm.algorithmId)).toEqual([
      'pump_status_telemetry__window_aggregate',
      'pump_status_telemetry__window_aggregate-2',
    ]);
    expect(materialized.algorithms[0].dsl.inputs[0].pointId).toBe('pump_pressure');
    expect(materialized.algorithms[1].dsl.inputs[0].pointId).toBe('pump_running');
  });

  it('persists conditional branch parameters and named ports for multiple MQTT outputs', () => {
    const template = JSON.parse(
      JSON.stringify(EDGE_CONFIG_TEMPLATES[1]),
    ) as (typeof EDGE_CONFIG_TEMPLATES)[number];
    template.dataConfig.visualGraph = {
      nodes: [
        { kind: 'point', label: 'pressure', nodeId: 'point-pump_pressure', refId: 'pump_pressure', x: 50, y: 120 },
        {
          kind: 'algorithm',
          label: '压力分支',
          nodeId: 'algorithm-condition_route',
          params: { operator: 'Gte', threshold: 80 },
          refId: 'condition_route',
          x: 320,
          y: 120,
        },
        { kind: 'mqtt', label: '高压主题', nodeId: 'mqtt-high', refId: 'factory/{edge_id}/pressure/high', x: 650, y: 60 },
        { kind: 'mqtt', label: '正常主题', nodeId: 'mqtt-normal', refId: 'factory/{edge_id}/pressure/normal', x: 650, y: 210 },
      ],
      edges: [
        {
          edgeId: 'pressure-route',
          from: 'point-pump_pressure',
          fromPort: 'value',
          to: 'algorithm-condition_route',
          toPort: 'input',
        },
        {
          edgeId: 'route-high',
          from: 'algorithm-condition_route',
          fromPort: 'matched',
          to: 'mqtt-high',
          toPort: 'payload',
        },
        {
          edgeId: 'route-normal',
          from: 'algorithm-condition_route',
          fromPort: 'unmatched',
          to: 'mqtt-normal',
          toPort: 'payload',
        },
      ],
    };

    const materialized = materializeProductRuntime(template, 'modbus-line-a');
    expect(materialized.algorithms).toHaveLength(1);
    expect(materialized.algorithms[0]).toMatchObject({
      algorithmKind: 'ThresholdRule',
      dsl: {
        outputs: [
          { name: 'matched', pointId: expect.stringContaining('.output.matched') },
          { name: 'unmatched', pointId: expect.stringContaining('.output.unmatched') },
        ],
        steps: [
          expect.objectContaining({
            matchedOutput: 'matched',
            operator: 'Gte',
            threshold: 80,
            type: 'conditionalRoute',
            unmatchedOutput: 'unmatched',
          }),
        ],
      },
    });

    const saved = buildProductVersionRequest(template, 'v1.4.4', []);
    const storedDataConfig = saved.dataConfigs[0] as { visual_graph: unknown };
    expect(storedDataConfig.visual_graph).toMatchObject({
      edges: expect.arrayContaining([
        expect.objectContaining({ from_port: 'matched', to: 'mqtt-high' }),
        expect.objectContaining({ from_port: 'unmatched', to: 'mqtt-normal' }),
      ]),
      nodes: expect.arrayContaining([
        expect.objectContaining({
          node_id: 'algorithm-condition_route',
          params: { operator: 'Gte', threshold: 80 },
        }),
      ]),
    });

    const hydrated = hydrateProductTemplate(
      template,
      {
        ...saved,
        createdAt: '2026-07-19T00:00:00Z',
        productId: template.templateId,
        status: 'draft',
      },
      [],
    );
    const restoredRoute = hydrated.dataConfig.visualGraph?.nodes.find(
      (node) => node.nodeId === 'algorithm-condition_route',
    );
    expect(restoredRoute?.refId).toBe('condition_route');
    expect(restoredRoute?.params).toEqual({ operator: 'Gte', threshold: 80 });
  });

  it('restores visual compute kinds after a product version save and reload', () => {
    const template = JSON.parse(
      JSON.stringify(EDGE_CONFIG_TEMPLATES[1]),
    ) as (typeof EDGE_CONFIG_TEMPLATES)[number];
    template.dataConfig.visualGraph = {
      nodes: [
        { kind: 'point', label: 'pressure', nodeId: 'point-pump_pressure', refId: 'pump_pressure', x: 50, y: 80 },
        { kind: 'point', label: 'running', nodeId: 'point-pump_running', refId: 'pump_running', x: 50, y: 180 },
        { kind: 'algorithm', label: '压力窗口', nodeId: 'algorithm-window_aggregate', refId: 'window_aggregate', x: 320, y: 80 },
        { kind: 'algorithm', label: '状态窗口', nodeId: 'algorithm-window_aggregate-2', refId: 'window_aggregate', x: 320, y: 180 },
        { kind: 'mqtt', label: '压力主题', nodeId: 'mqtt-pressure', refId: 'factory/{edge_id}/pressure', x: 650, y: 80 },
        { kind: 'mqtt', label: '状态主题', nodeId: 'mqtt-status', refId: 'factory/{edge_id}/status', x: 650, y: 180 },
      ],
      edges: [
        { edgeId: 'pressure-window', from: 'point-pump_pressure', to: 'algorithm-window_aggregate' },
        { edgeId: 'window-pressure', from: 'algorithm-window_aggregate', to: 'mqtt-pressure' },
        { edgeId: 'running-window', from: 'point-pump_running', to: 'algorithm-window_aggregate-2' },
        { edgeId: 'window-status', from: 'algorithm-window_aggregate-2', to: 'mqtt-status' },
      ],
    };

    const saved = buildProductVersionRequest(template, 'v1.4.4', []);
    const storedVersion: ProductVersionResponse = {
      ...saved,
      createdAt: '2026-07-15T00:00:00Z',
      productId: template.templateId,
      status: 'draft',
    };
    const hydrated = hydrateProductTemplate(template, storedVersion, []);

    expect(
      hydrated.dataConfig.visualGraph?.nodes
        .filter((node) => node.kind === 'algorithm')
        .map((node) => node.refId),
    ).toEqual(['window_aggregate', 'window_aggregate']);

    const rematerialized = materializeProductRuntime(hydrated, 'modbus-line-a');
    expect(rematerialized.algorithms.map((algorithm) => algorithm.algorithmKind)).toEqual([
      'WindowAggregate',
      'WindowAggregate',
    ]);
    expect(rematerialized.algorithms[0].dsl.inputs[0].pointId).toBe('pump_pressure');
    expect(rematerialized.algorithms[1].dsl.inputs[0].pointId).toBe('pump_running');
  });

  it('deduplicates stale version resources when saving the current product graph', () => {
    const template = JSON.parse(
      JSON.stringify(EDGE_CONFIG_TEMPLATES[0]),
    ) as (typeof EDGE_CONFIG_TEMPLATES)[number];
    template.versionResources = {
      algorithms: [
        { id: template.algorithm.algorithmId, kind: 'ChangeReport' },
        { id: template.algorithm.algorithmId, kind: 'ThresholdRule' },
      ],
      collectionTasks: [],
      dataConfigs: [],
      devices: [],
      mqttUplinks: [],
    };

    const saved = buildProductVersionRequest(template, 'v1.0.1', []);
    const matchingAlgorithms = saved.algorithms.filter(
      (algorithm) =>
        (algorithm as { id?: string }).id === template.algorithm.algorithmId,
    );

    expect(matchingAlgorithms).toHaveLength(1);
    expect(matchingAlgorithms[0]).toMatchObject({
      id: template.algorithm.algorithmId,
      kind: template.algorithm.algorithmKind,
    });
  });

  it('preserves published point node ids so inbound flow edges remain connected', () => {
    const template = JSON.parse(
      JSON.stringify(EDGE_CONFIG_TEMPLATES[1]),
    ) as (typeof EDGE_CONFIG_TEMPLATES)[number];
    const pressure = template.dataConfig.points[0];
    template.dataConfig.points = [pressure];
    const storedGraph = {
      nodes: [
        {
          kind: 'point' as const,
          label: '压力',
          nodeId: 'm-pressure',
          refId: pressure.pointId,
          x: 60,
          y: 40,
        },
        {
          kind: 'algorithm' as const,
          label: '压力窗口',
          nodeId: 'm-window',
          refId: 'window_aggregate',
          x: 340,
          y: 40,
        },
        {
          kind: 'mqtt' as const,
          label: '压力聚合',
          nodeId: 'm-mqtt-aggregate',
          refId: 'factory/{edge_id}/pressure',
          x: 680,
          y: 40,
        },
      ],
      edges: [
        {
          edgeId: 'm-pressure-window',
          from: 'm-pressure',
          fromPort: 'value',
          to: 'm-window',
          toPort: 'input',
        },
        {
          edgeId: 'm-window-out',
          from: 'm-window',
          fromPort: 'output',
          to: 'm-mqtt-aggregate',
          toPort: 'payload',
        },
      ],
    };

    const rebuilt = buildProductPlannerGraph(template, template.dataConfig, storedGraph);

    expect(rebuilt.nodes.find((node) => node.refId === pressure.pointId)?.nodeId)
      .toBe('m-pressure');
    expect(rebuilt.edges).toEqual(storedGraph.edges);
  });

  it('round-trips Siemens S7 and Omron FINS product connection settings', () => {
    const s7Template = JSON.parse(
      JSON.stringify(EDGE_CONFIG_TEMPLATES[1]),
    ) as (typeof EDGE_CONFIG_TEMPLATES)[number];
    s7Template.connection = {
      circuitBreaker: {
        enabled: true,
        failureThreshold: 8,
        halfOpenSuccessThreshold: 2,
        openDurationMs: 45000,
      },
      endpoint: 's7://192.168.10.20:102',
      protocolType: 'SiemensS7',
      siemensS7: {
        connectTimeoutMs: 4000,
        pduSize: 960,
        rack: 0,
        requestTimeoutMs: 7000,
        slot: 2,
      },
    };

    const savedS7 = buildProductVersionRequest(s7Template, 'v2.0.0', []);
    expect(savedS7.protocolConnections[0]).toMatchObject({
      circuit_breaker: {
        enabled: true,
        failure_threshold: 8,
        half_open_success_threshold: 2,
        open_duration_ms: 45000,
      },
      protocol: 'SiemensS7',
      siemens_s7: {
        connectTimeoutMs: 4000,
        pduSize: 960,
        rack: 0,
        requestTimeoutMs: 7000,
        slot: 2,
      },
    });
    const hydratedS7 = hydrateProductTemplate(
      s7Template,
      {
        ...savedS7,
        createdAt: '2026-08-03T00:00:00Z',
        productId: 'siemens-s7-pump-basic',
        status: 'draft',
      },
      [],
    );
    expect(hydratedS7.connection.siemensS7).toEqual(s7Template.connection.siemensS7);
    expect(hydratedS7.connection.circuitBreaker).toEqual(
      s7Template.connection.circuitBreaker,
    );

    const finsTemplate = JSON.parse(
      JSON.stringify(EDGE_CONFIG_TEMPLATES[1]),
    ) as (typeof EDGE_CONFIG_TEMPLATES)[number];
    finsTemplate.connection = {
      endpoint: 'fins://192.168.10.30:9600',
      omronFins: {
        destinationNetwork: 0,
        destinationNode: 12,
        destinationUnit: 0,
        sourceNetwork: 0,
        sourceNode: 7,
        sourceUnit: 0,
        timeoutMs: 3500,
        transport: 'tcp',
        wordOrder: 'high_word_first',
      },
      protocolType: 'OmronFins',
    };

    const savedFins = buildProductVersionRequest(finsTemplate, 'v2.0.0', []);
    expect(savedFins.protocolConnections[0]).toMatchObject({
      omron_fins: finsTemplate.connection.omronFins,
      protocol: 'OmronFins',
    });
    const hydratedFins = hydrateProductTemplate(
      finsTemplate,
      {
        ...savedFins,
        createdAt: '2026-08-03T00:00:00Z',
        productId: 'omron-fins-machine-basic',
        status: 'draft',
      },
      [],
    );
    expect(hydratedFins.connection.omronFins).toEqual(
      finsTemplate.connection.omronFins,
    );
  });

  it('preserves every industrial connection and data flow when a product version is saved', () => {
    const base = JSON.parse(
      JSON.stringify(EDGE_CONFIG_TEMPLATES[1]),
    ) as (typeof EDGE_CONFIG_TEMPLATES)[number];
    const version = {
      algorithms: [],
      collectionTasks: [
        {
          device_id: 'industrial-device',
          enabled: true,
          interval_ms: 1000,
          point_ids: ['modbus_pressure'],
          task_id: 'modbus-task',
        },
        {
          device_id: 'industrial-device',
          enabled: true,
          interval_ms: 500,
          point_ids: ['s7_speed'],
          task_id: 's7-task',
        },
      ],
      commandFlows: [],
      createdAt: '2026-08-05T00:00:00Z',
      dataConfigs: [
        {
          algorithm_ids: [],
          collection: { period_ms: 1000, retry_count: 2, timeout_ms: 800 },
          config_id: 'modbus-flow',
          device_id: 'industrial-device',
          enabled: true,
          name: 'Modbus 采集',
          points: [
            {
              address: { kind: 'holding_register', value: '0' },
              json_field: 'pressure',
              point_id: 'modbus_pressure',
              semantic_id: 'pump.pressure',
              value_type: 'Float',
            },
          ],
          protocol_connection_id: 'modbus-main',
          publish: { topic_template: 'factory/modbus' },
        },
        {
          algorithm_ids: [],
          collection: { period_ms: 500, retry_count: 2, timeout_ms: 800 },
          config_id: 's7-flow',
          device_id: 'industrial-device',
          enabled: true,
          name: 'S7 采集',
          points: [
            {
              address: { kind: 's7_db', value: 'DB1.DBD0' },
              json_field: 'speed',
              point_id: 's7_speed',
              semantic_id: 'drive.speed',
              value_type: 'Float',
            },
          ],
          protocol_connection_id: 's7-main',
          publish: { topic_template: 'factory/s7' },
        },
      ],
      deviceModels: [],
      devices: [{ device_id: 'industrial-device', device_type: 'industrial-line' }],
      mqttUplinks: [],
      pointSetIds: [],
      productId: 'industrial-product',
      protocolConnections: [
        {
          connection_id: 'modbus-main',
          endpoint: 'tcp://127.0.0.1:1502',
          protocol: 'ModbusTcp',
        },
        {
          connection_id: 's7-main',
          endpoint: 's7://127.0.0.1:11102',
          protocol: 'SiemensS7',
          siemens_s7: {
            connectTimeoutMs: 5000,
            pduSize: 480,
            rack: 0,
            requestTimeoutMs: 10000,
            slot: 1,
          },
        },
      ],
      status: 'draft' as const,
      version: 'v2.1.0',
    };

    const hydrated = hydrateProductTemplate(base, version, []);
    expect(hydrated.protocolConnections).toHaveLength(2);
    expect(hydrated.dataConfigBindings).toEqual([
      expect.objectContaining({
        configId: 'modbus-flow',
        protocolConnectionId: 'modbus-main',
      }),
      expect.objectContaining({ configId: 's7-flow', protocolConnectionId: 's7-main' }),
    ]);

    hydrated.dataConfigBindings = hydrated.dataConfigBindings?.map((binding) =>
      binding.configId === 's7-flow'
        ? { ...binding, protocolConnectionId: 's7-backup' }
        : binding,
    );
    hydrated.protocolConnections?.push({
      connectionId: 's7-backup',
      endpoint: 's7://127.0.0.1:21102',
      protocolType: 'SiemensS7',
      siemensS7: {
        connectTimeoutMs: 5000,
        pduSize: 480,
        rack: 0,
        requestTimeoutMs: 10000,
        slot: 1,
      },
    });

    const saved = buildProductVersionRequest(hydrated, 'v2.1.1', []);
    expect(saved.protocolConnections).toHaveLength(3);
    expect(saved.dataConfigs).toHaveLength(2);
    expect(saved.collectionTasks).toHaveLength(2);
    expect(saved.protocolConnections).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ connection_id: 'modbus-main' }),
        expect.objectContaining({ connection_id: 's7-main' }),
        expect.objectContaining({ connection_id: 's7-backup' }),
      ]),
    );
    expect(saved.dataConfigs).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          config_id: 'modbus-flow',
          protocol_connection_id: 'modbus-main',
        }),
        expect.objectContaining({
          config_id: 's7-flow',
          protocol_connection_id: 's7-backup',
        }),
      ]),
    );
  });

  it('persists project edits through the catalog API only when explicitly saved', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /项目管理/ }));
    expect(await screen.findByRole('heading', { level: 2, name: '项目管理' }))
      .toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '详情' }));

    const dialog = screen.getByRole('dialog', { name: '项目详情' });
    expect(within(dialog).getByRole('button', { name: '保存' })).toBeDisabled();
    expect(within(dialog).getByRole('button', { name: '删除' })).toBeDisabled();
    expect(within(dialog).getByRole('status')).toHaveTextContent('删除前需先清理');
    fireEvent.change(within(dialog).getByLabelText('项目名称'), {
      target: { value: '研发边缘项目' },
    });
    expect(within(dialog).getByRole('button', { name: '保存' })).toBeEnabled();
    expect(saveProject).not.toHaveBeenCalled();

    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));
    await waitFor(() => {
      expect(saveProject).toHaveBeenCalledWith(
        'demo-plant',
        expect.objectContaining({
          name: '研发边缘项目',
          projectId: 'demo-plant',
        }),
      );
    });
    expect(within(dialog).getByText('已保存')).toBeInTheDocument();

    fireEvent.click(within(dialog).getByText('关闭'));
    fireEvent.click(screen.getByRole('button', { name: '新建项目' }));
    await waitFor(() => expect(createProject).toHaveBeenCalledOnce());
    const createdProjectDialog = screen.getByRole('dialog', { name: '项目详情' });
    expect(await within(createdProjectDialog).findByDisplayValue('新项目 1')).toBeInTheDocument();
    fireEvent.click(within(createdProjectDialog).getByRole('button', { name: '删除' }));
    const projectConfirmation = within(createdProjectDialog).getByRole('alertdialog', {
      name: '确认删除项目',
    });
    fireEvent.click(within(projectConfirmation).getByRole('button', { name: '确认删除' }));
    await waitFor(() => expect(deleteProject).toHaveBeenCalledWith('project-1'));
    expect(screen.queryByRole('dialog', { name: '项目详情' })).not.toBeInTheDocument();
  });

  it('manages products from the dedicated product page', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /产品管理/ }));
    expect(await screen.findByRole('heading', { level: 2, name: '产品管理' })).toBeInTheDocument();
    expect(screen.getByText('产品列表')).toBeInTheDocument();
    expect(screen.queryByLabelText('产品名称')).not.toBeInTheDocument();

    fireEvent.click(screen.getAllByRole('button', { name: '配置' })[0]);
    const existingProductDialog = screen.getByRole('dialog', { name: '产品配置' });
    expect(existingProductDialog).toBeInTheDocument();
    expect(screen.getByLabelText('产品名称')).toBeInTheDocument();
    expect(
      within(existingProductDialog).getByRole('button', { name: '保存并同步' }),
    ).toBeDisabled();
    expect(within(existingProductDialog).getByRole('button', { name: '删除' })).toBeEnabled();

    fireEvent.change(screen.getByLabelText('产品名称'), {
      target: { value: '泵站默认配置产品' },
    });
    expect(saveProduct).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: '保存并同步' }));
    await waitFor(() => {
      expect(saveProduct).toHaveBeenCalledWith(
        'modbus-rtu-meter-basic',
        expect.objectContaining({ name: '泵站默认配置产品' }),
      );
      expect(createProductVersion).toHaveBeenCalledWith(
        'modbus-rtu-meter-basic',
        expect.objectContaining({ version: 'v1.2.1' }),
      );
    });
    expect(screen.getByRole('status')).toHaveTextContent(
      /已保存并触发自动同步|已保存，配置待完善/,
    );
    fireEvent.click(screen.getByText('关闭'));

    fireEvent.click(screen.getByRole('button', { name: '新建产品' }));
    const createdProductDialog = await screen.findByRole('dialog', { name: '产品配置' });
    expect(await within(createdProductDialog).findByDisplayValue(/自定义边端产品/)).toBeInTheDocument();
    fireEvent.click(within(createdProductDialog).getByRole('button', { name: '删除' }));
    const productConfirmation = within(createdProductDialog).getByRole('alertdialog', {
      name: '确认删除产品',
    });
    fireEvent.click(within(productConfirmation).getByRole('button', { name: '确认删除' }));
    await waitFor(() => expect(deleteProduct).toHaveBeenCalledWith(expect.stringMatching(/^custom-product-/)));
    expect(screen.queryByRole('dialog', { name: '产品配置' })).not.toBeInTheDocument();
  });

  it('uses save-and-sync without manual product release controls', async () => {
    render(<App />);
    fireEvent.click(screen.getByRole('button', { name: /产品管理/ }));
    await screen.findByText('产品列表');
    fireEvent.click(screen.getAllByRole('button', { name: '配置' })[0]);
    const dialog = screen.getByRole('dialog', { name: '产品配置' });
    expect(within(dialog).queryByRole('tab', { name: '发布策略' })).not.toBeInTheDocument();
    expect(within(dialog).queryByRole('button', { name: '发布此版本' })).not.toBeInTheDocument();
    expect(within(dialog).queryByRole('button', { name: '回滚到此版本' })).not.toBeInTheDocument();
    expect(within(dialog).getByRole('button', { name: '保存并同步' })).toBeInTheDocument();
  });

  it('configures product-bound points and collection orchestration in the product dialog', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /产品管理/ }));
    expect(await screen.findByText('产品列表')).toBeInTheDocument();
    fireEvent.click(screen.getAllByRole('button', { name: '配置' })[0]);

    const dialog = screen.getByRole('dialog', { name: '产品配置' });
    fireEvent.click(within(dialog).getByRole('tab', { name: '协议连接' }));
    fireEvent.click(within(dialog).getAllByRole('button', { name: '管理' })[0]);
    const workspace = screen.getByRole('dialog', { name: /协议连接工作区/ });
    fireEvent.click(within(workspace).getByRole('tab', { name: /绑定点位/ }));
    expect(within(workspace).getByText(/仅展示与当前协议兼容的点位集/)).toBeInTheDocument();
    fireEvent.click(within(workspace).getByRole('button', { name: '绑定' }));
    expect(within(dialog).getByText('有未保存修改')).toBeInTheDocument();

    fireEvent.click(within(workspace).getByRole('tab', { name: /采集编排/ }));
    expect(within(workspace).getByLabelText('采集编排资源')).toBeInTheDocument();
    expect(within(workspace).getByLabelText('采集编排画布')).toBeInTheDocument();
    expect(within(workspace).queryByRole('button', { name: '流程节点 JSON Payload' }))
      .not.toBeInTheDocument();
    expect(within(workspace).getByRole('button', { name: '流程节点 多点合并' }))
      .toBeInTheDocument();
    expect(workspace.querySelector('.node-red-canvas')).toBeInTheDocument();
    expect(workspace.querySelector('.node-red-wires')).toBeInTheDocument();
    expect(within(workspace).queryByRole('button', { name: '开始连线' })).not.toBeInTheDocument();
    fireEvent.click(within(workspace).getAllByRole('button', { name: /窗口聚合/ })[0]);
    expect(within(dialog).getByRole('button', { name: /流程节点 窗口聚合/ })).toBeInTheDocument();
    fireEvent.contextMenu(within(dialog).getByRole('button', { name: /流程节点 窗口聚合/ }));
    expect(within(dialog).getByRole('menuitem', { name: '编辑节点' })).toBeInTheDocument();
    expect(within(dialog).getByRole('menuitem', { name: '删除节点' })).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('menuitem', { name: '编辑节点' }));
    const inspector = dialog.querySelector('.node-red-edit-inspector') as HTMLElement;
    expect(within(inspector).getByRole('heading', { name: '窗口聚合' })).toBeInTheDocument();
    expect(within(inspector).getByRole('heading', { name: '计算设置' })).toBeInTheDocument();
    expect(within(inspector).queryByRole('heading', { name: '处理方式' })).not.toBeInTheDocument();
    expect(within(inspector).queryByLabelText('流水线 ID')).not.toBeInTheDocument();
    expect(within(inspector).queryByLabelText('采集周期(ms)')).not.toBeInTheDocument();
    expect(within(dialog).getByText('流程设置')).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: /流程节点 voltage_a/ }));
    expect(within(dialog).queryByLabelText('产品 JSON 字段 meter_voltage_a')).not.toBeInTheDocument();
    fireEvent.contextMenu(within(dialog).getByRole('button', { name: /流程节点 voltage_a/ }));
    fireEvent.click(within(dialog).getByRole('menuitem', { name: '编辑节点' }));
    expect(within(inspector).getByRole('heading', { name: 'voltage_a' })).toBeInTheDocument();
    expect(within(inspector).getByRole('heading', { name: '字段映射' })).toBeInTheDocument();
    expect(within(inspector).queryByText('连接')).not.toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: '从 voltage_a 连线' }));
    fireEvent.click(within(dialog).getByRole('button', { name: '连接到 多点合并' }));
    expect(
      within(dialog).getByLabelText(
        '删除连线 point-meter_voltage_a:value-to-algorithm-merge_points:input',
      ),
    ).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: '从 多点合并 连线' }));
    fireEvent.click(within(dialog).getByRole('button', { name: '连接到 窗口聚合' }));
    expect(dialog.querySelectorAll('.node-red-wires > g')).toHaveLength(2);
    fireEvent.click(within(dialog).getByRole('button', { name: '从 窗口聚合 连线' }));
    fireEvent.click(within(dialog).getByRole('button', { name: '连接到 多点合并' }));
    expect(dialog.querySelectorAll('.node-red-wires > g')).toHaveLength(2);
    fireEvent.click(within(dialog).getByRole('button', { name: /流程节点 voltage_a/ }));
    fireEvent.change(within(dialog).getByLabelText('产品 JSON 字段 meter_voltage_a'), {
      target: { value: 'ua' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '从 窗口聚合 连线' }));
    fireEvent.click(within(dialog).getByRole('button', { name: '连接到 MQTT 输出 1' }));
    fireEvent.contextMenu(within(dialog).getByRole('button', { name: '流程节点 MQTT 输出 1' }));
    fireEvent.click(within(dialog).getByRole('menuitem', { name: '编辑节点' }));
    fireEvent.change(within(dialog).getByLabelText('MQTT Topic'), {
      target: { value: 'factory/{edge_id}/pump/status' },
    });

    fireEvent.click(within(dialog).getByRole('button', { name: /MQTT 输出 拖入画布创建独立主题/ }));
    expect(within(dialog).getByText(/3 点位 \/ 2 计算节点 \/ 2 输出/)).toBeInTheDocument();
    expect(within(dialog).getByLabelText('MQTT JSON 结构')).toHaveValue('business');
    expect(within(dialog).getByLabelText('MQTT 附加时间戳')).not.toBeChecked();
    expect(within(dialog).getByLabelText('MQTT 附加质量信息')).not.toBeChecked();
    fireEvent.change(within(dialog).getByLabelText('MQTT 输出名称'), {
      target: { value: '告警主题' },
    });
    fireEvent.change(within(dialog).getByLabelText('MQTT Topic'), {
      target: { value: 'factory/{edge_id}/pump/alarm' },
    });

    const dragSource = within(dialog).getByRole('button', { name: '从 窗口聚合 连线' });
    const dragTarget = within(dialog).getByRole('button', { name: '连接到 告警主题' });
    const originalElementFromPoint = document.elementFromPoint;
    Object.defineProperty(document, 'elementFromPoint', {
      configurable: true,
      value: vi.fn(() => dragTarget),
    });
    const dispatchPointer = (type: string, clientX: number, clientY: number) => {
      const event = new MouseEvent(type, { bubbles: true, button: 0, clientX, clientY });
      Object.defineProperty(event, 'pointerId', { value: 7 });
      fireEvent(dragSource, event);
    };
    dispatchPointer('pointerdown', 500, 220);
    dispatchPointer('pointermove', 620, 300);
    expect(dialog.querySelector('.node-red-wire-preview')).toBeInTheDocument();
    dispatchPointer('pointerup', 760, 360);
    expect(dialog.querySelector('.node-red-wire-preview')).not.toBeInTheDocument();
    expect(
      within(dialog).getByLabelText(
        /删除连线 algorithm-window_aggregate:output-to-mqtt-output-\d+:payload/,
      ),
    ).toBeInTheDocument();
    Object.defineProperty(document, 'elementFromPoint', {
      configurable: true,
      value: originalElementFromPoint,
    });

    expect(within(dialog).queryByText('当前 DSL')).not.toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: '保存并同步' }));
    await waitFor(() => expect(createProductVersion).toHaveBeenCalledOnce());
    const savedRequest = vi.mocked(createProductVersion).mock.calls[0][1];
    const savedDataConfig = savedRequest.dataConfigs[0] as {
      points: Array<{ json_field: string; point_id: string }>;
      visual_graph: {
        edges: Array<{ from: string; from_port: string; to: string; to_port: string }>;
        nodes: Array<{
          kind: string;
          label: string;
          params?: Record<string, unknown>;
          ref_id: string;
        }>;
      };
    };
    expect(savedDataConfig.points).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ json_field: 'ua', point_id: 'meter_voltage_a' }),
      ]),
    );
    expect(savedDataConfig.visual_graph.nodes).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          kind: 'Mqtt',
          ref_id: 'factory/{edge_id}/pump/status',
        }),
        expect.objectContaining({
          kind: 'Mqtt',
          label: '告警主题',
          params: expect.objectContaining({
            includeQuality: false,
            includeTimestamp: false,
            payloadLayout: 'business',
          }),
          ref_id: 'factory/{edge_id}/pump/alarm',
        }),
      ]),
    );
    expect(savedDataConfig.visual_graph.edges).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ from_port: 'value', to_port: 'input' }),
        expect.objectContaining({ from_port: 'output', to_port: 'payload' }),
      ]),
    );
    expect(within(dialog).getByRole('status')).toHaveTextContent(
      /已保存并触发自动同步|已保存，配置待完善/,
    );
  });

  it('adds repeated compute nodes without automatically wiring them', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /产品管理/ }));
    await screen.findByText('产品列表');
    fireEvent.click(screen.getAllByRole('button', { name: '配置' })[0]);
    const dialog = screen.getByRole('dialog', { name: '产品配置' });
    fireEvent.click(within(dialog).getByRole('tab', { name: '协议连接' }));
    fireEvent.click(within(dialog).getAllByRole('button', { name: '管理' })[0]);
    const workspace = screen.getByRole('dialog', { name: /协议连接工作区/ });
    fireEvent.click(within(workspace).getByRole('tab', { name: /采集编排/ }));

    const paletteButton = within(workspace).getAllByRole('button', { name: /窗口聚合/ })[0];
    fireEvent.click(paletteButton);
    fireEvent.click(paletteButton);

    expect(within(dialog).getByRole('button', { name: '流程节点 窗口聚合' }))
      .toBeInTheDocument();
    expect(within(dialog).getByRole('button', { name: '流程节点 窗口聚合 2' }))
      .toBeInTheDocument();
    expect(dialog.querySelectorAll('.node-red-wires > g')).toHaveLength(0);
    expect(within(dialog).getByText(/2 点位 \/ 3 计算节点 \/ 1 输出/)).toBeInTheDocument();
  });

  it('persists continuous-condition nodes as executable Runtime DSL', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /产品管理/ }));
    await screen.findByText('产品列表');
    fireEvent.click(screen.getAllByRole('button', { name: '配置' })[0]);
    const dialog = screen.getByRole('dialog', { name: '产品配置' });
    fireEvent.click(within(dialog).getByRole('tab', { name: '协议连接' }));
    fireEvent.click(within(dialog).getAllByRole('button', { name: '管理' })[0]);
    const workspace = screen.getByRole('dialog', { name: /协议连接工作区/ });
    fireEvent.click(within(workspace).getByRole('tab', { name: /采集编排/ }));

    fireEvent.click(within(dialog).getAllByRole('button', { name: /持续条件/ })[0]);
    const node = within(dialog).getByRole('button', { name: '流程节点 持续条件' });
    fireEvent.contextMenu(node);
    fireEvent.click(within(dialog).getByRole('menuitem', { name: '编辑节点' }));

    fireEvent.change(within(dialog).getByLabelText('条件比较符'), {
      target: { value: 'Lte' },
    });
    fireEvent.change(within(dialog).getByLabelText('比较阈值'), {
      target: { value: '12.5' },
    });
    fireEvent.change(within(dialog).getByLabelText('持续时长(ms)'), {
      target: { value: '7500' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存并同步' }));

    await waitFor(() => expect(createProductVersion).toHaveBeenCalledOnce());
    const savedRequest = vi.mocked(createProductVersion).mock.calls[0][1];
    expect(savedRequest.algorithms).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          kind: 'DurationRule',
          dsl: expect.objectContaining({
            steps: [
              {
                durationMs: 7500,
                operator: 'Lte',
                output: 'value',
                source: 'p0',
                threshold: 12.5,
                type: 'durationCondition',
              },
            ],
          }),
        }),
      ]),
    );
  });

  it('keeps edge management focused on access, product binding, monitoring and token creation', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /边端管理/ }));
    expect(await screen.findByText('研发实验室边端')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '配置 edge-dev' })).not.toBeInTheDocument();
    expect(screen.getByText('泵站状态模板')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '接入信息 edge-dev' }));
    const accessDialog = screen.getByRole('dialog', { name: '边端接入信息' });
    expect(within(accessDialog).getByText('edge-dev')).toBeInTheDocument();
    expect(
      within(accessDialog).getByText('Token 仅在创建或重新生成时显示，Cloud 不保存明文。'),
    ).toBeInTheDocument();
    fireEvent.click(
      within(accessDialog).getByRole('button', { name: '重新生成 token' }),
    );
    expect(await within(accessDialog).findByText('edge_regenerated_secret')).toBeInTheDocument();
    expect(
      within(accessDialog).getByText(/edge-runtime --cloud-gateway-addr/),
    ).toBeInTheDocument();
    expect(generateEdgeAccessToken).toHaveBeenCalledWith('edge-dev');
    fireEvent.click(within(accessDialog).getByRole('button', { name: '关闭' }));

    fireEvent.click(screen.getByRole('button', { name: '新增边端' }));
    const createDialog = screen.getByRole('dialog', { name: '新增边端' });
    fireEvent.change(within(createDialog).getByLabelText('边端名称'), {
      target: { value: '产线 A 边端' },
    });
    fireEvent.change(within(createDialog).getByLabelText('站点/分组'), {
      target: { value: '产线/A' },
    });
    fireEvent.change(within(createDialog).getByLabelText('关联产品'), {
      target: { value: 'pump-collection-uplink' },
    });
    fireEvent.click(within(createDialog).getByRole('button', { name: '生成接入 token' }));

    await waitFor(() => {
      expect(createEdgeNode).toHaveBeenCalledWith({
        displayName: '产线 A 边端',
        productId: 'pump-collection-uplink',
        projectId: 'demo-plant',
        site: '产线/A',
      });
    });
    expect(await screen.findByText('已创建边端 edge-draft-1，token 已生成')).toBeInTheDocument();
    const createdAccessDialog = screen.getByRole('dialog', { name: '边端接入信息' });
    expect(within(createdAccessDialog).getByText('edge_created_secret')).toBeInTheDocument();
  });

  it('saves edge MQTT uplink overrides from edge management without opening product config', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /边端管理/ }));
    expect(await screen.findByText('研发实验室边端')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'MQTT 配置 edge-dev' }));

    const mqttDialog = screen.getByRole('dialog', { name: '边端 MQTT 配置' });
    fireEvent.change(within(mqttDialog).getByLabelText('传输安全'), {
      target: { value: 'mqtts' },
    });
    fireEvent.change(within(mqttDialog).getByLabelText('Broker 主机'), {
      target: { value: 'velamq.prod' },
    });
    fireEvent.click(within(mqttDialog).getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(saveMqttUplink).toHaveBeenCalledWith(
        'edge-dev',
        expect.objectContaining({
          broker: 'mqtts://velamq.prod:8883',
        }),
      );
    });
    expect(within(mqttDialog).getByText('已保存')).toBeInTheDocument();
  });

  it('opens selected edge runtime monitoring from the edge management row', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /边端管理/ }));
    expect(await screen.findByText('研发实验室边端')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '运行监控 edge-dev' }));

    const monitorDialog = await screen.findByRole('dialog', { name: '边端运行监控' });
    expect(within(monitorDialog).getByText('runtime-dev')).toBeInTheDocument();
    expect(within(monitorDialog).getByText('18.5%')).toBeInTheDocument();
  });

  it('keeps edge registration automatic from runtime connections', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /边端管理/ }));
    expect(await screen.findByText('研发实验室边端')).toBeInTheDocument();

    expect(screen.queryByRole('button', { name: '注册边端' })).not.toBeInTheDocument();
    expect(
      screen.getByText('手动登记边端，绑定产品，生成 runtime 接入 token。'),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '轮换凭证 edge-dev' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '维护模式 edge-dev' })).not.toBeInTheDocument();
  });

  it('renders dashboard as monitoring only without write actions', async () => {
    render(<App />);
    expect(
      await screen.findByRole('heading', { level: 1, name: 'Dashboard' }),
    ).toBeInTheDocument();

    expect(screen.queryByRole('button', { name: '注册边端' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '创建点位' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '发布配置' })).not.toBeInTheDocument();
    expect(screen.getByText('边端运行监控')).toBeInTheDocument();
    expect(screen.getByText('最近事件')).toBeInTheDocument();
    expect(screen.getByText('24ms')).toBeInTheDocument();
    expect(createPointMappingDraft).not.toHaveBeenCalled();
    expect(publishLatestRelease).not.toHaveBeenCalled();
  });

  it('runs agent actions through API clients without exposing model/discovery global navigation', async () => {
    render(<App />);

    expect(screen.queryByRole('button', { name: /设备模型/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /点位探测/ })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Agent 助手/ }));
    expect(await screen.findByText('云边配置助手')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '安全策略检查' }));
    await waitFor(() => {
      expect(runAgentSafetyCheck).toHaveBeenCalledOnce();
    });
    expect(await screen.findByText('安全策略结果')).toBeInTheDocument();
    expect(screen.getByText(/安全策略检查 已通过/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '生成候选建议' }));
    await waitFor(() => {
      expect(generateAgentSuggestions).toHaveBeenCalledOnce();
    });
    expect(screen.getAllByText('候选建议').length).toBeGreaterThan(0);
    expect(screen.getByText('已生成 1 条候选建议。建议只进入候选队列，不会自动修改配置。')).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('输入 Agent 问题'), {
      target: { value: '分析 edge-dev 状态' },
    });
    fireEvent.click(screen.getByRole('button', { name: '发送' }));
    await waitFor(() => expect(sendAgentChat).toHaveBeenCalledOnce());
    expect(await screen.findByText('本地分析')).toBeInTheDocument();
    expect(screen.getByText('当前边端健康，建议先校验配置差异。')).toBeInTheDocument();
  });

  it('loads runtime status into the runtime monitoring page', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /运行状态/ }));

    expect((await screen.findAllByText('edge-dev')).length).toBeGreaterThan(0);
    expect(screen.getAllByText('24ms').length).toBeGreaterThan(0);
    expect(screen.getByText('Modbus TCP')).toBeInTheDocument();
  });

});
