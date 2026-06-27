import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  createEdgeNode,
  fetchAlgorithms,
  fetchAuditRecords,
  fetchCollectionTasks,
  fetchDeviceModels,
  fetchEdgeAlgorithms,
  fetchEdgeCollectionTasks,
  fetchEdgePointMappings,
  fetchEdgeProtocolConnections,
  fetchEdgeNodes,
  fetchPointMappings,
  fetchProtocolConnections,
  fetchReleaseList,
  fetchRuntimeStatus,
  fetchSummary,
  publishLatestRelease,
  createEdgeProtocolConnection,
  rotateEdgeCredentials,
  enableEdgeMaintenanceMode,
  saveEdgeAlgorithm,
  saveEdgeCollectionTask,
  saveEdgePointMapping,
  saveEdgeProtocolConnection,
} from './api/client';
import type {
  AlgorithmResponse,
  AuditRecordResponse,
  CollectionTaskResponse,
  DeviceModelResponse,
  EdgeNodeResponse,
  PointMappingResponse,
  ProtocolConnectionResponse,
  ReleaseListResponse,
  RuntimeStatusResponse,
  SaveAlgorithmRequest,
  SaveCollectionTaskRequest,
  SaveProtocolConnectionRequest,
} from './api/types';
import App from './App';

vi.mock('./api/client', () => ({
  createEdgeNode: vi.fn(),
  fetchAlgorithms: vi.fn(),
  fetchAuditRecords: vi.fn(),
  fetchCollectionTasks: vi.fn(),
  fetchDeviceModels: vi.fn(),
  fetchEdgeAlgorithms: vi.fn(),
  fetchEdgeCollectionTasks: vi.fn(),
  fetchEdgePointMappings: vi.fn(),
  fetchEdgeProtocolConnections: vi.fn(),
  fetchEdgeNodes: vi.fn(),
  fetchPointMappings: vi.fn(),
  fetchProtocolConnections: vi.fn(),
  fetchReleaseList: vi.fn(),
  fetchRuntimeStatus: vi.fn(),
  fetchSummary: vi.fn(),
  publishLatestRelease: vi.fn(),
  createEdgeProtocolConnection: vi.fn(),
  rotateEdgeCredentials: vi.fn(),
  enableEdgeMaintenanceMode: vi.fn(),
  saveEdgeAlgorithm: vi.fn(),
  saveEdgeCollectionTask: vi.fn(),
  saveEdgePointMapping: vi.fn(),
  saveEdgeProtocolConnection: vi.fn(),
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

const registeredEdgeNode: EdgeNodeResponse = {
  edgeId: 'edge-draft-2',
  displayName: '新边端注册草稿',
  site: '待分配',
  runtimeId: '-',
  status: '未上报',
  resources: '-',
  heartbeat: '-',
  capabilities: ['registration:draft'],
};

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

const algorithms: AlgorithmResponse[] = [
  {
    edgeId: 'edge-dev',
    algorithmId: 'pump-anomaly-v1',
    version: '1.0.0',
    runtime: 'Onnx',
    kind: '异常检测',
    inputIds: ['pressure', 'running'],
    outputIds: ['pump.anomaly_score'],
    inputs: 'pressure, running',
    outputs: 'pump.anomaly_score',
    execution: '边端本地执行',
    validation: '已通过',
  },
];

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
    vi.mocked(fetchAlgorithms).mockResolvedValue(algorithms);
    vi.mocked(fetchEdgeAlgorithms).mockResolvedValue(algorithms);
    vi.mocked(fetchReleaseList).mockResolvedValue(initialReleaseList);
    vi.mocked(fetchRuntimeStatus).mockResolvedValue(runtimeStatus);
    vi.mocked(fetchAuditRecords).mockResolvedValue(auditRecords);
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
    vi.mocked(saveEdgeProtocolConnection).mockResolvedValue({
      ...protocolConnections[0],
      endpoint: 'opc.tcp://10.12.0.80:4840',
      protocol: 'OPC UA',
      protocolType: 'OpcUa',
    });
    vi.mocked(createEdgeProtocolConnection).mockResolvedValue(createdProtocolConnection);
    vi.mocked(createEdgeNode).mockResolvedValue(registeredEdgeNode);
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
      inputIds: ['pressure'],
      inputs: 'pressure',
      kind: 'WASM 算法',
      outputIds: ['pump.pressure_score'],
      outputs: 'pump.pressure_score',
      runtime: 'Wasm',
      version: '1.1.0',
    });
    vi.mocked(publishLatestRelease).mockResolvedValue(updatedReleaseList);
  });

  it('saves point drafts through the API and refreshes point mappings', async () => {
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

    fireEvent.change(screen.getByLabelText('地址值'), {
      target: { value: '40002' },
    });
    fireEvent.change(screen.getByLabelText('采集周期(ms)'), {
      target: { value: '2000' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存草稿' }));

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
    expect(screen.getByText('草稿已保存')).toBeInTheDocument();
  });

  it('saves collection task drafts through the selected edge API', async () => {
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

    fireEvent.change(screen.getByLabelText('采集点位'), {
      target: { value: 'pressure' },
    });
    fireEvent.change(screen.getByLabelText('采集周期(ms)'), {
      target: { value: '2500' },
    });
    fireEvent.click(screen.getByLabelText('启用任务'));
    fireEvent.click(screen.getByRole('button', { name: '保存草稿' }));

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
    expect(screen.getByText('草稿已保存')).toBeInTheDocument();
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
    expect(await screen.findByText('10.12.0.20:502')).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('协议类型'), {
      target: { value: 'OpcUa' },
    });
    fireEvent.change(screen.getByLabelText('端点'), {
      target: { value: 'opc.tcp://10.12.0.80:4840' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存草稿' }));

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
    expect(screen.getByText('草稿已保存')).toBeInTheDocument();
  });

  it('creates protocol connection drafts through the selected edge API', async () => {
    vi.mocked(fetchEdgeProtocolConnections)
      .mockResolvedValueOnce(protocolConnections)
      .mockResolvedValueOnce(protocolConnections)
      .mockResolvedValueOnce([...protocolConnections, createdProtocolConnection]);

    render(<App />);

    await openEdgeConfiguration();
    expect(await screen.findByText('10.12.0.20:502')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '新建连接' }));

    await waitFor(() => {
      expect(createEdgeProtocolConnection).toHaveBeenCalledWith('edge-dev', {
        endpoint: null,
        protocolType: 'ModbusTcp',
      });
    });
    expect(
      await screen.findByRole('button', { name: '选择连接 connection-draft-2' }),
    ).toBeInTheDocument();
    expect(screen.getByText('已创建连接草稿 connection-draft-2')).toBeInTheDocument();
  });

  it('keeps configuration sections as lists from the sidebar', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /协议连接/ }));
    expect(await screen.findByText('10.12.0.20:502')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '新建连接' })).not.toBeInTheDocument();
    expect(screen.queryByText('编辑连接 modbus-line-a')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /点位配置/ }));
    expect(await screen.findByText('holding_register:40001')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '新建点位' })).not.toBeInTheDocument();
    expect(screen.queryByText('编辑点位 pressure')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /采集任务/ }));
    expect(await screen.findByText('pump-main')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '新建任务' })).not.toBeInTheDocument();
    expect(screen.queryByText('编辑任务 pump-main')).not.toBeInTheDocument();
  });

  it('opens selected edge configuration from the edge management row', async () => {
    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /边端管理/ }));
    expect(await screen.findByText('研发实验室边端')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '配置边端 edge-dev' }));

    expect(await screen.findByText('编辑连接 modbus-line-a')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '新建连接' })).toBeInTheDocument();
    await waitFor(() => {
      expect(fetchEdgeProtocolConnections).toHaveBeenCalledWith('edge-dev');
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

  it('registers edge drafts through the edge management API', async () => {
    vi.mocked(fetchEdgeNodes)
      .mockResolvedValueOnce(edgeNodes)
      .mockResolvedValueOnce([...edgeNodes, registeredEdgeNode]);
    vi.mocked(fetchSummary)
      .mockResolvedValueOnce({
        edge_count: 1,
        pending_release_count: 0,
      })
      .mockResolvedValueOnce({
        edge_count: 2,
        pending_release_count: 0,
      });

    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /边端管理/ }));
    expect(await screen.findByText('研发实验室边端')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '注册边端' }));

    await waitFor(() => {
      expect(createEdgeNode).toHaveBeenCalledWith({
        displayName: '新边端注册草稿',
        site: '待分配',
      });
    });
    expect(await screen.findByText('edge-draft-2')).toBeInTheDocument();
    expect(screen.getByText('已注册边端草稿 edge-draft-2')).toBeInTheDocument();
  });

  it('runs credential rotation and maintenance mode through edge APIs', async () => {
    vi.mocked(fetchEdgeNodes)
      .mockResolvedValueOnce(edgeNodes)
      .mockResolvedValueOnce(edgeNodes)
      .mockResolvedValueOnce(maintenanceEdgeNodes);

    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /边端管理/ }));
    expect(await screen.findByText('研发实验室边端')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '轮换凭证' }));
    await waitFor(() => {
      expect(rotateEdgeCredentials).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findByText('凭证已轮换 credential-v2')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '维护模式' }));
    await waitFor(() => {
      expect(enableEdgeMaintenanceMode).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findByText('维护模式已启用 维护中')).toBeInTheDocument();
    expect(screen.getByText('维护中')).toBeInTheDocument();
  });

  it('saves algorithm drafts through the selected edge API', async () => {
    vi.mocked(fetchEdgeAlgorithms)
      .mockResolvedValueOnce(algorithms)
      .mockResolvedValueOnce(algorithms)
      .mockResolvedValueOnce([
        {
          ...algorithms[0],
          inputIds: ['pressure'],
          inputs: 'pressure',
          kind: 'WASM 算法',
          outputIds: ['pump.pressure_score'],
          outputs: 'pump.pressure_score',
          runtime: 'Wasm',
          version: '1.1.0',
        },
      ]);

    render(<App />);

    await openEdgeConfiguration();
    fireEvent.click(screen.getByRole('button', { name: /算法配置/ }));
    expect(await screen.findByText('pressure, running')).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('算法版本'), {
      target: { value: '1.1.0' },
    });
    fireEvent.change(screen.getByLabelText('算法运行时'), {
      target: { value: 'Wasm' },
    });
    fireEvent.change(screen.getByLabelText('输入点位'), {
      target: { value: 'pressure' },
    });
    fireEvent.change(screen.getByLabelText('输出变量'), {
      target: { value: 'pump.pressure_score' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存草稿' }));

    const expectedRequest: SaveAlgorithmRequest = {
      version: '1.1.0',
      runtime: 'Wasm',
      inputIds: ['pressure'],
      outputIds: ['pump.pressure_score'],
    };
    await waitFor(() => {
      expect(saveEdgeAlgorithm).toHaveBeenCalledWith(
        'edge-dev',
        'pump-anomaly-v1',
        expectedRequest,
      );
    });
    expect(await screen.findByText('pump.pressure_score')).toBeInTheDocument();
    expect(screen.getByText('草稿已保存')).toBeInTheDocument();
  });

  it('publishes the latest draft and refreshes release apply results', async () => {
    vi.mocked(fetchReleaseList)
      .mockResolvedValueOnce(initialReleaseList)
      .mockResolvedValueOnce(updatedReleaseList);

    render(<App />);

    fireEvent.click(screen.getByRole('button', { name: /配置发布/ }));
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
    expect(await screen.findByText('pump@v1 遥测定义')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /协议连接/ }));
    expect(await screen.findByText('10.12.0.20:502')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /采集任务/ }));
    expect(await screen.findByText('pump-main')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /算法配置/ }));
    expect(await screen.findByText('pump-anomaly-v1')).toBeInTheDocument();

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

    fireEvent.click(screen.getByRole('button', { name: /协议连接/ }));

    await waitFor(() => {
      expect(fetchEdgeProtocolConnections).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findAllByText('modbus-line-a')).toHaveLength(1);
    expect(screen.queryByText('opc.tcp://historical-draft:4840')).not.toBeInTheDocument();
  });
});

async function openEdgeConfiguration() {
  fireEvent.click(screen.getByRole('button', { name: /边端管理/ }));
  expect(await screen.findByText('研发实验室边端')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: '配置边端 edge-dev' }));
  expect(await screen.findByText('编辑连接 modbus-line-a')).toBeInTheDocument();
}
