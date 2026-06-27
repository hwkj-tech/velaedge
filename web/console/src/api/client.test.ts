import { describe, expect, it, vi } from 'vitest';

import {
  fetchAlgorithms,
  fetchAuditRecords,
  fetchCollectionTasks,
  fetchDeviceModels,
  fetchEdgeCollectionTasks,
  fetchEdgeNodes,
  fetchEdgePointMappings,
  fetchPointMappings,
  fetchProtocolConnections,
  fetchReleaseList,
  fetchRuntimeStatus,
  fetchSummary,
  publishLatestRelease,
  saveEdgeCollectionTask,
  saveEdgePointMapping,
  savePointMapping,
} from './client';

describe('fetchSummary', () => {
  it('loads cloud summary from the API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ edge_count: 2, pending_release_count: 1 }),
    });

    const result = await fetchSummary(fetchMock as unknown as typeof fetch);

    expect(result.edge_count).toBe(2);
    expect(result.pending_release_count).toBe(1);
  });
});

describe('fetchPointMappings', () => {
  it('loads point mappings from the API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => [
        {
          edgeId: 'edge-dev',
          pointId: 'pressure',
          pointName: 'pressure',
          deviceId: 'pump-1',
          deviceModel: 'pump',
          semanticTelemetry: 'pump.pressure',
          protocol: 'Modbus TCP',
          connection: 'modbus-line-a',
          address: 'holding_register:40001',
          valueType: 'float32',
          readWrite: 'read',
          unit: 'MPa',
          scale: '1',
          interval: '1000ms',
          range: '-',
          qualityRule: 'timeout->bad',
          status: '启用',
        },
      ],
    });

    const result = await fetchPointMappings(fetchMock as unknown as typeof fetch);

    expect(fetchMock).toHaveBeenCalledWith('/api/point-mappings');
    expect(result[0].pointId).toBe('pressure');
    expect(result[0].address).toBe('holding_register:40001');
  });
});

describe('fetchEdgePointMappings', () => {
  it('loads point mappings for the selected edge from the API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => [
        {
          edgeId: 'edge-dev',
          pointId: 'running',
          pointName: 'running',
          deviceId: 'pump-1',
          deviceModel: 'pump',
          semanticTelemetry: 'pump.running',
          protocol: 'Modbus TCP',
          connection: 'modbus-line-a',
          address: 'coil:00001',
          valueType: 'bool',
          readWrite: 'read',
          unit: '-',
          scale: '1',
          interval: '1000ms',
          range: '-',
          qualityRule: 'timeout->bad',
          status: '启用',
        },
      ],
    });

    const result = await fetchEdgePointMappings(
      'edge-dev',
      fetchMock as unknown as typeof fetch,
    );

    expect(fetchMock).toHaveBeenCalledWith('/api/edges/edge-dev/point-mappings');
    expect(result[0].edgeId).toBe('edge-dev');
    expect(result[0].pointId).toBe('running');
  });
});

describe('fetchReleaseList', () => {
  it('loads release apply results from the API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
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
      }),
    });

    const result = await fetchReleaseList(fetchMock as unknown as typeof fetch);

    expect(fetchMock).toHaveBeenCalledWith('/api/releases');
    expect(result.draftVersion).toBe('2026.06.26-001');
    expect(result.applyResults[0].edgeId).toBe('edge-dev');
  });
});

describe('fetchRuntimeStatus', () => {
  it('loads runtime metrics and events from the API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
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
            protocols: [],
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
      }),
    });

    const result = await fetchRuntimeStatus(fetchMock as unknown as typeof fetch);

    expect(fetchMock).toHaveBeenCalledWith('/api/runtime-status');
    expect(result.edges[0].edge_id).toBe('edge-dev');
    expect(result.averageCollectionLatencyMs).toBe(24);
  });
});

describe('management data clients', () => {
  it('loads API-backed management lists', async () => {
    const payloads: Record<string, unknown> = {
      '/api/edge-nodes': [{ edgeId: 'edge-dev' }],
      '/api/device-models': [{ deviceType: 'pump' }],
      '/api/protocol-connections': [{ connectionId: 'modbus-line-a' }],
      '/api/collection-tasks': [{ taskId: 'pump-main' }],
      '/api/algorithms': [{ algorithmId: 'pump-anomaly-v1' }],
      '/api/audit-records': [{ action: 'create_release' }],
    };
    const fetchMock = vi.fn().mockImplementation((path: string) =>
      Promise.resolve({
        ok: true,
        json: async () => payloads[path],
      }),
    );

    await expect(fetchEdgeNodes(fetchMock as unknown as typeof fetch)).resolves.toEqual(
      payloads['/api/edge-nodes'],
    );
    await expect(fetchDeviceModels(fetchMock as unknown as typeof fetch)).resolves.toEqual(
      payloads['/api/device-models'],
    );
    await expect(
      fetchProtocolConnections(fetchMock as unknown as typeof fetch),
    ).resolves.toEqual(payloads['/api/protocol-connections']);
    await expect(
      fetchCollectionTasks(fetchMock as unknown as typeof fetch),
    ).resolves.toEqual(payloads['/api/collection-tasks']);
    await expect(fetchAlgorithms(fetchMock as unknown as typeof fetch)).resolves.toEqual(
      payloads['/api/algorithms'],
    );
    await expect(fetchAuditRecords(fetchMock as unknown as typeof fetch)).resolves.toEqual(
      payloads['/api/audit-records'],
    );

    expect(fetchMock).toHaveBeenCalledWith('/api/edge-nodes');
    expect(fetchMock).toHaveBeenCalledWith('/api/device-models');
    expect(fetchMock).toHaveBeenCalledWith('/api/protocol-connections');
    expect(fetchMock).toHaveBeenCalledWith('/api/collection-tasks');
    expect(fetchMock).toHaveBeenCalledWith('/api/algorithms');
    expect(fetchMock).toHaveBeenCalledWith('/api/audit-records');
  });
});

describe('fetchEdgeCollectionTasks', () => {
  it('loads collection tasks for the selected edge from the API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => [
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
      ],
    });

    const result = await fetchEdgeCollectionTasks(
      'edge-dev',
      fetchMock as unknown as typeof fetch,
    );

    expect(fetchMock).toHaveBeenCalledWith('/api/edges/edge-dev/collection-tasks');
    expect(result[0].taskId).toBe('pump-main');
    expect(result[0].pointIds).toEqual(['pressure', 'running']);
  });
});

describe('savePointMapping', () => {
  it('sends an editable point mapping draft to the API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        pointId: 'pressure/main',
        address: 'holding_register:40002',
        interval: '2000ms',
      }),
    });

    const result = await savePointMapping(
      'pressure/main',
      {
        addressKind: 'holding_register',
        addressValue: '40002',
        intervalMs: 2000,
        unit: 'MPa',
      },
      fetchMock as unknown as typeof fetch,
    );

    expect(fetchMock).toHaveBeenCalledWith('/api/point-mappings/pressure%2Fmain', {
      body: JSON.stringify({
        addressKind: 'holding_register',
        addressValue: '40002',
        intervalMs: 2000,
        unit: 'MPa',
      }),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    });
    expect(result.address).toBe('holding_register:40002');
  });
});

describe('saveEdgeCollectionTask', () => {
  it('sends an editable collection task draft to the selected edge API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        edgeId: 'edge-dev',
        taskId: 'pump-main',
        deviceId: 'pump-1',
        pointIds: ['pressure'],
        pointList: 'pressure',
        intervalMs: 2500,
        interval: '2500ms',
        enabled: false,
        status: '暂停',
      }),
    });

    const result = await saveEdgeCollectionTask(
      'edge-dev',
      'pump-main',
      {
        deviceId: 'pump-1',
        pointIds: ['pressure'],
        intervalMs: 2500,
        enabled: false,
      },
      fetchMock as unknown as typeof fetch,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/edges/edge-dev/collection-tasks/pump-main',
      {
        body: JSON.stringify({
          deviceId: 'pump-1',
          pointIds: ['pressure'],
          intervalMs: 2500,
          enabled: false,
        }),
        headers: { 'content-type': 'application/json' },
        method: 'PUT',
      },
    );
    expect(result.status).toBe('暂停');
  });
});

describe('saveEdgePointMapping', () => {
  it('sends an editable point mapping draft to the selected edge API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        edgeId: 'edge-dev',
        pointId: 'pressure/main',
        address: 'holding_register:40002',
        interval: '2000ms',
      }),
    });

    const result = await saveEdgePointMapping(
      'edge-dev',
      'pressure/main',
      {
        addressKind: 'holding_register',
        addressValue: '40002',
        intervalMs: 2000,
        unit: 'MPa',
      },
      fetchMock as unknown as typeof fetch,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/edges/edge-dev/point-mappings/pressure%2Fmain',
      {
        body: JSON.stringify({
          addressKind: 'holding_register',
          addressValue: '40002',
          intervalMs: 2000,
          unit: 'MPa',
        }),
        headers: { 'content-type': 'application/json' },
        method: 'PUT',
      },
    );
    expect(result.edgeId).toBe('edge-dev');
    expect(result.address).toBe('holding_register:40002');
  });
});

describe('publishLatestRelease', () => {
  it('publishes the latest cloud draft for the selected edge and returns apply results', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        draftVersion: '2026.06.26-002',
        validationStatus: '已通过',
        changeSummary: '云端配置包已生成',
        rolloutPolicy: '单边端发布',
        applyResults: [
          {
            edgeId: 'edge-dev',
            desiredVersion: '2026.06.26-002',
            reportedVersion: '-',
            result: '等待下发',
            heartbeat: '18 秒前',
          },
        ],
      }),
    });

    const result = await publishLatestRelease(
      'edge-dev',
      fetchMock as unknown as typeof fetch,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/edges/edge-dev/releases/publish',
      {
        method: 'POST',
      },
    );
    expect(result.draftVersion).toBe('2026.06.26-002');
    expect(result.applyResults[0].result).toBe('等待下发');
  });
});
