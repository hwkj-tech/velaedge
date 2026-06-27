import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  fetchAlgorithms,
  fetchAuditRecords,
  fetchCollectionTasks,
  fetchDeviceModels,
  fetchEdgeNodes,
  fetchPointMappings,
  fetchProtocolConnections,
  fetchReleaseList,
  fetchRuntimeStatus,
  fetchSummary,
  publishLatestRelease,
  savePointMapping,
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
} from './api/types';
import App from './App';

vi.mock('./api/client', () => ({
  fetchAlgorithms: vi.fn(),
  fetchAuditRecords: vi.fn(),
  fetchCollectionTasks: vi.fn(),
  fetchDeviceModels: vi.fn(),
  fetchEdgeNodes: vi.fn(),
  fetchPointMappings: vi.fn(),
  fetchProtocolConnections: vi.fn(),
  fetchReleaseList: vi.fn(),
  fetchRuntimeStatus: vi.fn(),
  fetchSummary: vi.fn(),
  publishLatestRelease: vi.fn(),
  savePointMapping: vi.fn(),
}));

const basePoint: PointMappingResponse = {
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
    endpoint: '10.12.0.20:502',
    status: '启用',
    policy: '1000ms timeout / 3 retry',
  },
];

const collectionTasks: CollectionTaskResponse[] = [
  {
    edgeId: 'edge-dev',
    taskId: 'pump-main',
    deviceId: 'pump-1',
    pointList: 'pressure, running',
    interval: '1000ms',
    status: '启用',
  },
];

const algorithms: AlgorithmResponse[] = [
  {
    edgeId: 'edge-dev',
    algorithmId: 'pump-anomaly-v1',
    version: '1.0.0',
    kind: '异常检测',
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
    vi.mocked(fetchPointMappings).mockResolvedValue([basePoint]);
    vi.mocked(fetchCollectionTasks).mockResolvedValue(collectionTasks);
    vi.mocked(fetchAlgorithms).mockResolvedValue(algorithms);
    vi.mocked(fetchReleaseList).mockResolvedValue(initialReleaseList);
    vi.mocked(fetchRuntimeStatus).mockResolvedValue(runtimeStatus);
    vi.mocked(fetchAuditRecords).mockResolvedValue(auditRecords);
    vi.mocked(savePointMapping).mockResolvedValue({
      ...basePoint,
      address: 'holding_register:40002',
      interval: '2000ms',
    });
    vi.mocked(publishLatestRelease).mockResolvedValue(updatedReleaseList);
  });

  it('saves point drafts through the API and refreshes point mappings', async () => {
    vi.mocked(fetchPointMappings)
      .mockResolvedValueOnce([basePoint])
      .mockResolvedValueOnce([
        {
          ...basePoint,
          address: 'holding_register:40002',
          interval: '2000ms',
        },
      ]);

    render(<App />);

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
      expect(savePointMapping).toHaveBeenCalledWith('pressure', {
        addressKind: 'holding_register',
        addressValue: '40002',
        intervalMs: 2000,
        unit: 'MPa',
      });
    });
    expect(await screen.findByText('holding_register:40002')).toBeInTheDocument();
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
});
