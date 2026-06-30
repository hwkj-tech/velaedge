import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  createAlgorithmDraft,
  createCollectionTaskDraft,
  createEdgeDataConfig,
  createDeviceModelDraft,
  createPointMappingDraft,
  fetchAlgorithms,
  fetchAuditRecords,
  fetchCollectionTasks,
  fetchDeviceModels,
  fetchEdgeAlgorithms,
  fetchEdgeCollectionTasks,
  fetchEdgeDataConfigs,
  fetchEdgePointMappings,
  fetchEdgeProtocolConnections,
  fetchEdgeNodes,
  fetchPointMappings,
  fetchProtocolConnections,
  fetchReleaseList,
  fetchRuntimeStatus,
  fetchSummary,
  fetchMqttUplink,
  fetchDiscoverySuggestions,
  generateAgentSuggestions,
  publishLatestRelease,
  runDiscovery,
  runAgentSafetyCheck,
  runConfigValidation,
  runReleaseDiff,
  createEdgeProtocolConnection,
  deleteEdgeDataConfig,
  rotateEdgeCredentials,
  enableEdgeMaintenanceMode,
  saveEdgeAlgorithm,
  saveEdgeCollectionTask,
  saveEdgeDataConfig,
  saveEdgePointMapping,
  saveEdgeProtocolConnection,
  saveMqttUplink,
} from './api/client';
import type {
  AlgorithmResponse,
  AuditRecordResponse,
  CollectionTaskResponse,
  DataConfigResponse,
  DeviceModelResponse,
  EdgeNodeResponse,
  PointMappingResponse,
  ProtocolConnectionResponse,
  ReleaseListResponse,
  RuntimeStatusResponse,
  SaveAlgorithmRequest,
  SaveCollectionTaskRequest,
  SaveDataConfigRequest,
  SaveProtocolConnectionRequest,
} from './api/types';
import App from './App';

vi.mock('./api/client', () => ({
  createAlgorithmDraft: vi.fn(),
  createCollectionTaskDraft: vi.fn(),
  createEdgeDataConfig: vi.fn(),
  createDeviceModelDraft: vi.fn(),
  createPointMappingDraft: vi.fn(),
  fetchAlgorithms: vi.fn(),
  fetchAuditRecords: vi.fn(),
  fetchCollectionTasks: vi.fn(),
  fetchDeviceModels: vi.fn(),
  fetchEdgeAlgorithms: vi.fn(),
  fetchEdgeCollectionTasks: vi.fn(),
  fetchEdgeDataConfigs: vi.fn(),
  fetchEdgePointMappings: vi.fn(),
  fetchEdgeProtocolConnections: vi.fn(),
  fetchEdgeNodes: vi.fn(),
  fetchPointMappings: vi.fn(),
  fetchProtocolConnections: vi.fn(),
  fetchReleaseList: vi.fn(),
  fetchRuntimeStatus: vi.fn(),
  fetchSummary: vi.fn(),
  fetchMqttUplink: vi.fn(),
  fetchDiscoverySuggestions: vi.fn(),
  generateAgentSuggestions: vi.fn(),
  publishLatestRelease: vi.fn(),
  runDiscovery: vi.fn(),
  runAgentSafetyCheck: vi.fn(),
  runConfigValidation: vi.fn(),
  runReleaseDiff: vi.fn(),
  createEdgeProtocolConnection: vi.fn(),
  deleteEdgeDataConfig: vi.fn(),
  rotateEdgeCredentials: vi.fn(),
  enableEdgeMaintenanceMode: vi.fn(),
  saveEdgeAlgorithm: vi.fn(),
  saveEdgeCollectionTask: vi.fn(),
  saveEdgeDataConfig: vi.fn(),
  saveEdgePointMapping: vi.fn(),
  saveEdgeProtocolConnection: vi.fn(),
  saveMqttUplink: vi.fn(),
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

const createdDeviceModel: DeviceModelResponse = {
  commandCount: 0,
  deviceType: 'meter',
  eventCount: 0,
  telemetry: [
    {
      description: 'A 相电压',
      name: 'voltage_a',
      range: '0-500',
      telemetryId: 'voltage_a',
      unit: 'V',
      valueType: 'float32',
    },
  ],
  version: 'v2',
};

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

describe('App cloud console write actions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(fetchSummary).mockResolvedValue({
      edge_count: 1,
      pending_release_count: 0,
    });
    vi.mocked(fetchEdgeNodes).mockResolvedValue(edgeNodes);
    vi.mocked(fetchDeviceModels).mockResolvedValue(deviceModels);
    vi.mocked(fetchProtocolConnections).mockResolvedValue(protocolConnections);
    vi.mocked(fetchEdgeProtocolConnections).mockResolvedValue(protocolConnections);
    vi.mocked(fetchPointMappings).mockResolvedValue([basePoint]);
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
    vi.mocked(createAlgorithmDraft).mockResolvedValue(createdAlgorithm);
    vi.mocked(createCollectionTaskDraft).mockResolvedValue(createdCollectionTask);
    vi.mocked(createDeviceModelDraft).mockResolvedValue(createdDeviceModel);
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
    vi.mocked(rotateEdgeCredentials).mockResolvedValue({
      action: 'rotate_credentials',
      credentialVersion: 'credential-v2',
      edgeId: 'edge-dev',
      message: '凭证已轮换',
    });
    vi.mocked(enableEdgeMaintenanceMode).mockResolvedValue({
      action: 'enable_maintenance',
      edgeId: 'edge-dev',
      message: '维护模式已启用',
      status: '维护中',
    });
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

  it.skip('saves point drafts through the API and refreshes point mappings', async () => {
    vi.mocked(fetchEdgePointMappings)
      .mockResolvedValueOnce([basePoint])
      .mockResolvedValueOnce([basePoint])
      .mockResolvedValueOnce([
        {
          ...basePoint,
          address: 'holding_register:40002',
          interval: '2000ms',
        },
      ]);

    render(<App />);

    await openEdgeConfiguration();
    fireEvent.click(screen.getByRole('button', { name: /点位配置/ }));
    expect(await screen.findByText('holding_register:40001')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '选择点位 pressure' }));

    fireEvent.change(screen.getByLabelText('地址值'), {
      target: { value: '40002' },
    });
    fireEvent.change(screen.getByLabelText('采集周期(ms)'), {
      target: { value: '2000' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(saveEdgePointMapping).toHaveBeenCalledWith(
        'edge-dev',
        'pressure',
        {
          addressKind: 'holding_register',
          addressValue: '40002',
          intervalMs: 2000,
          unit: 'MPa',
        },
      );
    });
    expect(await screen.findByText('holding_register:40002')).toBeInTheDocument();
    expect(screen.getByText('已保存')).toBeInTheDocument();
  });

  it.skip('saves collection task drafts through the selected edge API', async () => {
    vi.mocked(fetchEdgeCollectionTasks)
      .mockResolvedValueOnce(collectionTasks)
      .mockResolvedValueOnce(collectionTasks)
      .mockResolvedValueOnce([
        {
          ...collectionTasks[0],
          enabled: false,
          interval: '2500ms',
          intervalMs: 2500,
          pointIds: ['pressure'],
          pointList: 'pressure',
          status: '暂停',
        },
      ]);

    render(<App />);

    await openEdgeConfiguration();
    fireEvent.click(screen.getByRole('button', { name: /采集任务/ }));
    expect((await screen.findAllByText('pressure, running')).length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole('button', { name: '选择任务 pump-main' }));

    fireEvent.change(screen.getByLabelText('采集点位'), {
      target: { value: 'pressure' },
    });
    fireEvent.change(screen.getByLabelText('采集周期(ms)'), {
      target: { value: '2500' },
    });
    fireEvent.click(screen.getByLabelText('启用任务'));
    fireEvent.click(screen.getByRole('button', { name: '保存' }));

    const expectedRequest: SaveCollectionTaskRequest = {
      deviceId: 'pump-1',
      enabled: false,
      intervalMs: 2500,
      pointIds: ['pressure'],
    };
    await waitFor(() => {
      expect(saveEdgeCollectionTask).toHaveBeenCalledWith(
        'edge-dev',
        'pump-main',
        expectedRequest,
      );
    });
    expect((await screen.findAllByText('pressure')).length).toBeGreaterThan(0);
    expect(screen.getByText('已保存')).toBeInTheDocument();
  });

  it('saves data configs through the selected edge API', async () => {
    vi.mocked(fetchEdgeDataConfigs)
      .mockResolvedValueOnce(dataConfigs)
      .mockResolvedValueOnce(dataConfigs)
      .mockResolvedValueOnce([{ ...dataConfigs[0], name: '泵状态' }]);

    render(<App />);

    await openEdgeConfiguration();
    fireEvent.click(screen.getByRole('tab', { name: '数据上报' }));
    fireEvent.click(screen.getByRole('button', { name: 'pump_status' }));
    const dialog = screen.getByRole('dialog', { name: '编辑数据上报' });
    fireEvent.change(within(dialog).getByLabelText('配置名称'), {
      target: { value: '泵状态' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '下一步' }));
    fireEvent.click(within(dialog).getByRole('button', { name: '下一步' }));
    fireEvent.click(within(dialog).getByRole('button', { name: '下一步' }));
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(saveEdgeDataConfig).toHaveBeenCalledWith(
        'edge-dev',
        'pump_status',
        expect.objectContaining({
          configId: 'pump_status',
          name: '泵状态',
          visualGraph: expect.objectContaining({
            nodes: expect.arrayContaining([
              expect.objectContaining({ kind: 'json' }),
              expect.objectContaining({ kind: 'mqtt' }),
            ]),
          }),
        } satisfies Partial<SaveDataConfigRequest>),
      );
    });
    expect(await screen.findByText('泵状态')).toBeInTheDocument();
  });

  it('saves protocol connection drafts through the selected edge API', async () => {
    vi.mocked(fetchEdgeProtocolConnections)
      .mockResolvedValueOnce(protocolConnections)
      .mockResolvedValueOnce(protocolConnections)
      .mockResolvedValueOnce([
        {
          ...protocolConnections[0],
          endpoint: 'opc.tcp://10.12.0.80:4840',
          protocol: 'OPC UA',
          protocolType: 'OpcUa',
        },
      ]);

    render(<App />);

    await openEdgeConfiguration();
    fireEvent.click(screen.getByRole('tab', { name: '协议连接' }));
    expect(await screen.findByText('10.12.0.20:502')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '选择连接 modbus-line-a' }));

    fireEvent.change(screen.getByLabelText('协议类型'), {
      target: { value: 'OpcUa' },
    });
    fireEvent.change(screen.getByLabelText('端点'), {
      target: { value: 'opc.tcp://10.12.0.80:4840' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));

    const expectedRequest: SaveProtocolConnectionRequest = {
      endpoint: 'opc.tcp://10.12.0.80:4840',
      protocolType: 'OpcUa',
    };
    await waitFor(() => {
      expect(saveEdgeProtocolConnection).toHaveBeenCalledWith(
        'edge-dev',
        'modbus-line-a',
        expectedRequest,
      );
    });
    expect(await screen.findByText('opc.tcp://10.12.0.80:4840')).toBeInTheDocument();
    expect(screen.getByText('已保存')).toBeInTheDocument();
  });

  it('creates protocol connection drafts through the selected edge API', async () => {
    vi.mocked(fetchEdgeProtocolConnections)
      .mockResolvedValueOnce(protocolConnections)
      .mockResolvedValueOnce(protocolConnections)
      .mockResolvedValueOnce([...protocolConnections, createdProtocolConnection]);

    render(<App />);

    await openEdgeConfiguration();
    fireEvent.click(screen.getByRole('tab', { name: '协议连接' }));
    expect(await screen.findByText('10.12.0.20:502')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '新建连接' }));
    const dialog = screen.getByRole('dialog', { name: '新建协议连接' });
    fireEvent.change(within(dialog).getByLabelText('新建协议类型'), {
      target: { value: 'ModbusRtu' },
    });
    fireEvent.change(within(dialog).getByLabelText('新建端点'), {
      target: { value: '/dev/ttyUSB1' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(createEdgeProtocolConnection).toHaveBeenCalledWith('edge-dev', {
        endpoint: '/dev/ttyUSB1',
        protocolType: 'ModbusRtu',
      });
    });
    expect(
      await screen.findByRole('button', { name: '选择连接 connection-draft-2' }),
    ).toBeInTheDocument();
    expect(screen.getByText('已创建连接 connection-draft-2')).toBeInTheDocument();
  });

  it('opens configuration sections as editable management tabs from the selected edge', async () => {
    render(<App />);

    expect(screen.queryByRole('button', { name: /协议连接/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /数据上报/ })).not.toBeInTheDocument();

    await openEdgeConfiguration();
    fireEvent.click(screen.getByRole('button', { name: '维护连接' }));
    expect(await screen.findByText('10.12.0.20:502')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '新建连接' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '选择连接 modbus-line-a' }));
    expect(screen.getByText('编辑连接 modbus-line-a')).toBeInTheDocument();
    expect(screen.queryByLabelText('当前边端')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('配置边端')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '保存草稿' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '关闭' }));

    fireEvent.click(screen.getByRole('tab', { name: '数据上报' }));
    expect(await screen.findByRole('button', { name: 'pump_status' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '新建数据上报' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'pump_status' }));
    expect(screen.getByText('编辑数据上报 pump_status')).toBeInTheDocument();
    expect(screen.queryByLabelText('当前边端')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('配置边端')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '关闭' }));

    expect(screen.queryByRole('button', { name: /算法配置/ })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'pump_status' }));
    const dataConfigDialog = screen.getByRole('dialog', { name: '编辑数据上报' });
    fireEvent.click(within(dataConfigDialog).getByRole('button', { name: '2. 可视化编排' }));
    expect(within(dataConfigDialog).getByText('pump-anomaly-v1')).toBeInTheDocument();
  });

  it('keeps editable controls when switching configuration tabs', async () => {
    render(<App />);

    await openEdgeConfiguration();
    fireEvent.click(screen.getByRole('tab', { name: '数据上报' }));
    expect(screen.getByRole('button', { name: 'pump_status' })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('tab', { name: '协议连接' }));

    expect(await screen.findByText('10.12.0.20:502')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '选择连接 modbus-line-a' }));
    expect(screen.getByText('编辑连接 modbus-line-a')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '新建连接' })).toBeInTheDocument();
  });

  it('opens selected edge configuration from the edge management row', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /边端管理/ }));
    expect(await screen.findByText('研发实验室边端')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '选择边端配置 edge-dev' }));
    expect(screen.getByRole('dialog', { name: '选择边端配置' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '打开配置总览' }));

    expect(await screen.findByRole('heading', { name: '研发实验室边端' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: '配置总览' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('button', { name: '维护上报' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '维护上报' }));
    expect(await screen.findByRole('button', { name: 'pump_status' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '新建数据上报' })).toBeInTheDocument();
    await waitFor(() => {
      expect(fetchEdgeDataConfigs).toHaveBeenCalledWith('edge-dev');
    });
  });

  it('opens selected edge runtime monitoring from the edge management row', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /边端管理/ }));
    expect(await screen.findByText('研发实验室边端')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '运行监控 edge-dev' }));

    expect(await screen.findByText('正在监控 edge-dev')).toBeInTheDocument();
    expect(screen.getByText('18.5%')).toBeInTheDocument();
  });

  it('keeps edge registration automatic from runtime connections', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /边端管理/ }));
    expect(await screen.findByText('研发实验室边端')).toBeInTheDocument();

    expect(screen.queryByRole('button', { name: '注册边端' })).not.toBeInTheDocument();
    expect(
      screen.getByText(
        '边端由 runtime 通过 EdgeLink 主动连接后自动登记。云端负责查看运行状态、进入边端配置，并维护该边端的 MQTT 上报连接。',
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '轮换凭证 edge-dev' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '维护模式 edge-dev' })).not.toBeInTheDocument();
  });

  it.skip('saves algorithm drafts through the selected edge API', async () => {
    vi.mocked(fetchEdgeAlgorithms)
      .mockResolvedValueOnce(algorithms)
      .mockResolvedValueOnce(algorithms)
      .mockResolvedValueOnce([
        {
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
        },
      ]);

    render(<App />);

    await openEdgeConfiguration();
    fireEvent.click(screen.getByRole('button', { name: /算法配置/ }));
    expect(await screen.findByText('pressure.reported')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '选择算法 pump-anomaly-v1' }));

    fireEvent.change(screen.getByLabelText('算法版本'), {
      target: { value: '1.1.0' },
    });
    fireEvent.change(screen.getByLabelText('算法类型'), {
      target: { value: 'WindowAggregate' },
    });
    fireEvent.change(screen.getByLabelText('输入点位'), {
      target: { value: 'pressure' },
    });
    fireEvent.change(screen.getByLabelText('输出虚拟点位'), {
      target: { value: 'pressure.avg_1m' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));

    const expectedRequest: SaveAlgorithmRequest = {
      version: '1.1.0',
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
    };
    await waitFor(() => {
      expect(saveEdgeAlgorithm).toHaveBeenCalledWith(
        'edge-dev',
        'pump-anomaly-v1',
        expectedRequest,
      );
    });
    expect(await screen.findByText('pressure.avg_1m')).toBeInTheDocument();
    expect(screen.getByText('已保存')).toBeInTheDocument();
  });

  it('publishes the latest draft and refreshes release apply results', async () => {
    vi.mocked(fetchReleaseList)
      .mockResolvedValueOnce(initialReleaseList)
      .mockResolvedValueOnce(updatedReleaseList);

    render(<App />);

    await openEdgeConfiguration();
    fireEvent.click(screen.getByRole('tab', { name: '配置发布' }));
    await waitFor(() => {
      expect(screen.getAllByText('2026.06.26-001').length).toBeGreaterThan(0);
    });

    fireEvent.click(screen.getByRole('button', { name: '创建发布' }));

    await waitFor(() => {
      expect(publishLatestRelease).toHaveBeenCalledWith('edge-dev');
    });
    await waitFor(() => {
      expect(screen.getAllByText('2026.06.26-002').length).toBeGreaterThan(0);
    });
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

  it.skip('creates point, task, and algorithm drafts from edge configuration pages', async () => {
    vi.mocked(fetchEdgePointMappings)
      .mockResolvedValueOnce([basePoint])
      .mockResolvedValueOnce([basePoint])
      .mockResolvedValueOnce([basePoint, createdPoint]);
    vi.mocked(fetchEdgeCollectionTasks)
      .mockResolvedValueOnce(collectionTasks)
      .mockResolvedValueOnce(collectionTasks)
      .mockResolvedValueOnce([...collectionTasks, createdCollectionTask]);
    vi.mocked(fetchEdgeAlgorithms)
      .mockResolvedValueOnce(algorithms)
      .mockResolvedValueOnce(algorithms)
      .mockResolvedValueOnce([...algorithms, createdAlgorithm]);

    render(<App />);
    await openEdgeConfiguration();

    fireEvent.click(screen.getByRole('button', { name: /点位配置/ }));
    expect(await screen.findByText('holding_register:40001')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '新建点位' }));
    const pointDialog = screen.getByRole('dialog', { name: '新建点位' });
    fireEvent.change(within(pointDialog).getByLabelText('新建 Point ID'), {
      target: { value: 'temperature' },
    });
    fireEvent.change(within(pointDialog).getByLabelText('新建设备 ID'), {
      target: { value: 'pump-1' },
    });
    fireEvent.change(within(pointDialog).getByLabelText('新建语义遥测'), {
      target: { value: 'pump.temperature' },
    });
    fireEvent.change(within(pointDialog).getByLabelText('新建连接实例'), {
      target: { value: 'modbus-line-a' },
    });
    fireEvent.change(within(pointDialog).getByLabelText('新建地址值'), {
      target: { value: '30001' },
    });
    fireEvent.click(within(pointDialog).getByRole('button', { name: '保存' }));
    await waitFor(() => {
      expect(createPointMappingDraft).toHaveBeenCalledWith('edge-dev', {
        addressKind: 'holding_register',
        addressValue: '30001',
        connectionId: 'modbus-line-a',
        deviceId: 'pump-1',
        intervalMs: 1000,
        pointId: 'temperature',
        semanticId: 'pump.temperature',
        unit: '-',
        valueType: 'float32',
      });
    });
    expect(await screen.findByText('point-draft-2')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /采集任务/ }));
    expect(
      await screen.findByRole('button', { name: '选择任务 pump-main' }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '新建任务' }));
    const taskDialog = screen.getByRole('dialog', { name: '新建采集任务' });
    fireEvent.change(within(taskDialog).getByLabelText('新建 Task ID'), {
      target: { value: 'thermal-task' },
    });
    fireEvent.change(within(taskDialog).getByLabelText('新建任务设备 ID'), {
      target: { value: 'pump-1' },
    });
    fireEvent.change(within(taskDialog).getByLabelText('新建任务采集点位'), {
      target: { value: 'pressure' },
    });
    fireEvent.click(within(taskDialog).getByRole('button', { name: '保存' }));
    await waitFor(() => {
      expect(createCollectionTaskDraft).toHaveBeenCalledWith('edge-dev', {
        deviceId: 'pump-1',
        enabled: true,
        intervalMs: 1000,
        pointIds: ['pressure'],
        taskId: 'thermal-task',
      });
    });
    expect(await screen.findByText('task-draft-2')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('tab', { name: '数据上报' }));
    fireEvent.click(await screen.findByRole('button', { name: 'pump_status' }));
    fireEvent.click(screen.getByRole('button', { name: '2. 可视化编排' }));
    expect(screen.getByText('pump-anomaly-v1')).toBeInTheDocument();
  });

  it.skip('imports discovered point suggestions into selected edge point mappings', async () => {
    const importSuggestions = [
      {
        pointId: 'meter_voltage_a',
        deviceId: 'meter-1',
        semanticId: 'electric.voltage_a',
        protocolConnectionId: 'meter-rs485-bus-1',
        address: 'holding_register:40001',
        valueType: 'float32',
        unit: 'V',
        confidence: 0.82,
        evidence: '旧候选连接不存在',
      },
      {
        pointId: 'flow_rate',
        deviceId: 'pump-1',
        semanticId: 'pump.flow_rate',
        protocolConnectionId: 'modbus-line-a',
        address: 'holding_register:40003',
        valueType: 'float32',
        unit: 'm3/h',
        confidence: 0.88,
        evidence: '流量读数稳定',
      },
    ];
    vi.mocked(fetchDiscoverySuggestions)
      .mockResolvedValueOnce(discoverySuggestions)
      .mockResolvedValueOnce(importSuggestions)
      .mockResolvedValueOnce(importSuggestions);
    vi.mocked(fetchEdgePointMappings)
      .mockResolvedValueOnce([basePoint])
      .mockResolvedValueOnce([basePoint])
      .mockResolvedValueOnce([basePoint])
      .mockResolvedValueOnce([
        basePoint,
        {
          ...createdPoint,
          pointId: 'flow_rate',
          pointName: 'flow_rate',
          semanticTelemetry: 'pump.flow_rate',
        },
      ]);

    render(<App />);
    await openEdgeConfiguration();

    fireEvent.click(screen.getByRole('button', { name: /点位配置/ }));
    expect(await screen.findByText('holding_register:40001')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '批量导入' }));

    await waitFor(() => {
      expect(fetchDiscoverySuggestions).toHaveBeenCalledWith('edge-dev');
    });
    await waitFor(() => {
      expect(createPointMappingDraft).toHaveBeenCalledTimes(1);
      expect(createPointMappingDraft).toHaveBeenCalledWith('edge-dev', {
        addressKind: 'holding_register',
        addressValue: '40003',
        connectionId: 'modbus-line-a',
        deviceId: 'pump-1',
        pointId: 'flow_rate',
        semanticId: 'pump.flow_rate',
        unit: 'm3/h',
        valueType: 'float32',
      });
    });
    expect(await screen.findByText('已导入 1 个候选点位')).toBeInTheDocument();
  });

  it('runs validation and release diff actions through API clients', async () => {
    render(<App />);
    await openEdgeConfiguration();

    fireEvent.click(screen.getByRole('tab', { name: '配置发布' }));
    expect(
      await screen.findByRole('heading', { name: '配置发布', level: 2 }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '校验配置' }));
    await waitFor(() => {
      expect(runConfigValidation).toHaveBeenCalledWith('edge-dev');
    });
    fireEvent.click(screen.getByRole('button', { name: '查看差异' }));
    await waitFor(() => {
      expect(runReleaseDiff).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findByText('配置差异摘要已生成')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '校验配置' }));
    await waitFor(() => {
      expect(runConfigValidation).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findByText('发布配置校验 已通过')).toBeInTheDocument();
  });

  it('creates device model drafts and runs agent actions through API clients', async () => {
    vi.mocked(fetchDeviceModels)
      .mockResolvedValueOnce(deviceModels)
      .mockResolvedValueOnce([...deviceModels, createdDeviceModel]);

    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /设备模型/ }));
    expect(
      await screen.findByRole('button', { name: '选择设备模型 pump' }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '新建设备模型' }));
    const dialog = screen.getByRole('dialog', { name: '新建设备模型' });
    expect(dialog).toBeInTheDocument();
    fireEvent.change(within(dialog).getByLabelText('设备类型'), {
      target: { value: 'meter' },
    });
    fireEvent.change(within(dialog).getByLabelText('模型版本'), {
      target: { value: 'v2' },
    });
    fireEvent.change(within(dialog).getByLabelText('遥测 ID'), {
      target: { value: 'voltage_a' },
    });
    fireEvent.change(within(dialog).getByLabelText('数据类型'), {
      target: { value: 'float32' },
    });
    fireEvent.change(within(dialog).getByLabelText('单位'), {
      target: { value: 'V' },
    });
    fireEvent.change(within(dialog).getByLabelText('范围'), {
      target: { value: '0-500' },
    });
    fireEvent.change(within(dialog).getByLabelText('说明'), {
      target: { value: 'A 相电压' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存设备模型' }));
    await waitFor(() => {
      expect(createDeviceModelDraft).toHaveBeenCalledWith({
        deviceType: 'meter',
        version: 'v2',
        telemetry: [
          {
            description: 'A 相电压',
            range: '0-500',
            telemetryId: 'voltage_a',
            unit: 'V',
            valueType: 'float32',
          },
        ],
      });
    });
    expect(await screen.findByText('已创建设备模型 meter@v2')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Agent 助手/ }));
    expect(await screen.findByText('Agent 辅助管理')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '安全策略' }));
    await waitFor(() => {
      expect(runAgentSafetyCheck).toHaveBeenCalledOnce();
    });
    expect(await screen.findByText('安全策略检查 已通过')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '生成建议' }));
    await waitFor(() => {
      expect(generateAgentSuggestions).toHaveBeenCalledOnce();
    });
    expect(await screen.findByText('Agent 建议已生成 1 条')).toBeInTheDocument();
  });

  it('loads runtime status into the runtime monitoring page', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /运行状态/ }));

    expect((await screen.findAllByText('edge-dev')).length).toBeGreaterThan(0);
    expect(screen.getAllByText('24ms').length).toBeGreaterThan(0);
    expect(screen.getByText('Modbus TCP')).toBeInTheDocument();
  });

  it('loads backend management lists into formerly static pages', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /边端管理/ }));
    expect(await screen.findByText('研发实验室边端')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /设备模型/ }));
    expect(
      await screen.findByRole('button', { name: '选择设备模型 pump' }),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /边端管理/ }));
    fireEvent.click(screen.getByRole('button', { name: '选择边端配置 edge-dev' }));
    fireEvent.click(screen.getByRole('button', { name: '选择连接' }));
    expect(await screen.findByText('10.12.0.20:502')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('tab', { name: '数据上报' }));
    expect(await screen.findByRole('button', { name: 'pump_status' })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /审计日志/ }));
    expect(await screen.findByText('create_release')).toBeInTheDocument();
  });

  it('loads edge-scoped configuration lists on first render', async () => {
    vi.mocked(fetchProtocolConnections).mockResolvedValueOnce([
      protocolConnections[0],
      {
        ...protocolConnections[0],
        endpoint: 'opc.tcp://historical-draft:4840',
      },
    ]);
    vi.mocked(fetchEdgeProtocolConnections).mockResolvedValueOnce(protocolConnections);

    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /边端管理/ }));
    expect(await screen.findByText('研发实验室边端')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '选择边端配置 edge-dev' }));
    fireEvent.click(screen.getByRole('button', { name: '选择连接' }));

    await waitFor(() => {
      expect(fetchEdgeProtocolConnections).toHaveBeenCalledWith('edge-dev');
    });
    expect(
      await screen.findByRole('button', { name: '选择连接 modbus-line-a' }),
    ).toBeInTheDocument();
    expect(screen.queryByText('opc.tcp://historical-draft:4840')).not.toBeInTheDocument();
  });
});

async function openEdgeConfiguration() {
  fireEvent.click(screen.getByRole('button', { name: /边端管理/ }));
  expect(await screen.findByText('研发实验室边端')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: '选择边端配置 edge-dev' }));
  expect(screen.getByRole('dialog', { name: '选择边端配置' })).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: '打开配置总览' }));
  expect(await screen.findByRole('heading', { name: '研发实验室边端' })).toBeInTheDocument();
  expect(screen.getByRole('tab', { name: '配置总览' })).toHaveAttribute('aria-selected', 'true');
  expect(screen.getByText('配置绑定总览')).toBeInTheDocument();
}
