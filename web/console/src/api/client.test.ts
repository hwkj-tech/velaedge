import { describe, expect, it, vi } from 'vitest';

import {
  createAlgorithmDraft,
  createEdgeNode,
  createDeviceModelDraft,
  createCollectionTaskDraft,
  createPointMappingDraft,
  createEdgeProtocolConnection,
  enableEdgeMaintenanceMode,
  fetchDiscoverySuggestions,
  generateAgentSuggestions,
  fetchAlgorithms,
  fetchAuditRecords,
  fetchCollectionTasks,
  fetchDeviceModels,
  fetchEdgeCollectionTasks,
  fetchEdgeAlgorithms,
  fetchEdgeNodes,
  fetchEdgePointMappings,
  fetchEdgeProtocolConnections,
  fetchPointMappings,
  fetchProtocolConnections,
  fetchReleaseList,
  fetchRuntimeStatus,
  fetchMqttUplink,
  fetchSummary,
  runAgentSafetyCheck,
  runConfigValidation,
  runReleaseDiff,
  runDiscovery,
  saveMqttUplink,
  publishLatestRelease,
  rotateEdgeCredentials,
  saveEdgeCollectionTask,
  saveEdgeAlgorithm,
  saveDeviceModel,
  saveEdgePointMapping,
  saveEdgeProtocolConnection,
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

describe('mqtt uplink and discovery clients', () => {
  it('loads and saves mqtt uplink config for an edge', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        sinkId: 'velamq-main',
        broker: 'mqtts://velamq.local:8883',
        clientId: 'edge-dev-runtime-dev',
        topicTemplate: 'edge/{edge_id}/device/{device_id}/telemetry',
        qos: 1,
        batchSize: 100,
        flushIntervalMs: 1000,
      }),
    });

    const uplink = await fetchMqttUplink(
      'edge-dev',
      fetchMock as unknown as typeof fetch,
    );
    expect(fetchMock).toHaveBeenCalledWith('/api/edges/edge-dev/mqtt-uplink');
    expect(uplink.broker).toBe('mqtts://velamq.local:8883');

    await saveMqttUplink('edge-dev', uplink, fetchMock as unknown as typeof fetch);
    expect(fetchMock).toHaveBeenLastCalledWith('/api/edges/edge-dev/mqtt-uplink', {
      body: JSON.stringify(uplink),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    });
  });

  it('runs discovery and loads agent mapping suggestions', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        jobId: 'discovery-edge-dev-1',
        protocolConnectionId: 'meter-rs485-bus-1',
        discoveredPoints: [],
        suggestions: [
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
        ],
      }),
    });

    const report = await runDiscovery(
      'edge-dev',
      {
        addressRange: 'holding_register:40001-40002',
        connectionId: 'meter-rs485-bus-1',
      },
      fetchMock as unknown as typeof fetch,
    );
    expect(fetchMock).toHaveBeenCalledWith('/api/edges/edge-dev/discovery/run', {
      body: JSON.stringify({
        addressRange: 'holding_register:40001-40002',
        connectionId: 'meter-rs485-bus-1',
      }),
      headers: { 'content-type': 'application/json' },
      method: 'POST',
    });
    expect(report.suggestions[0].pointId).toBe('meter_voltage_a');

    await fetchDiscoverySuggestions('edge-dev', fetchMock as unknown as typeof fetch);
    expect(fetchMock).toHaveBeenLastCalledWith(
      '/api/edges/edge-dev/discovery/suggestions',
    );
  });
});

describe('management data clients', () => {
  it('loads API-backed management lists', async () => {
    const payloads: Record<string, unknown> = {
      '/api/edge-nodes': [{ edgeId: 'edge-dev' }],
      '/api/device-models': [{ deviceType: 'pump' }],
      '/api/protocol-connections': [
        { connectionId: 'modbus-line-a', protocolType: 'ModbusTcp' },
      ],
      '/api/collection-tasks': [{ taskId: 'pump-main' }],
      '/api/algorithms': [
        { algorithmId: 'pump-anomaly-v1', runtime: 'Onnx', inputIds: ['pressure'] },
      ],
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

describe('fetchEdgeProtocolConnections', () => {
  it('loads protocol connections for the selected edge from the API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => [
        {
          edgeId: 'edge-dev',
          connectionId: 'modbus-line-a',
          protocolType: 'ModbusTcp',
          protocol: 'Modbus TCP',
          endpoint: '10.12.0.20:502',
          status: '启用',
          policy: '1000ms timeout / 3 retry',
        },
      ],
    });

    const result = await fetchEdgeProtocolConnections(
      'edge-dev',
      fetchMock as unknown as typeof fetch,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/edges/edge-dev/protocol-connections',
    );
    expect(result[0].protocolType).toBe('ModbusTcp');
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

describe('fetchEdgeAlgorithms', () => {
  it('loads algorithms for the selected edge from the API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => [
        {
          edgeId: 'edge-dev',
          algorithmId: 'pump-anomaly-v1',
          version: '1.0.0',
          algorithmKind: 'ChangeReport',
          dsl: {
            inputs: [{ alias: 'p', pointId: 'pressure' }],
            trigger: { type: 'onSample' },
            steps: [{ type: 'changeFilter', source: 'p', threshold: 0.2 }],
            outputs: [{ name: 'reported', pointId: 'pressure.reported' }],
            report: { mode: 'OnChange', sink: 'velamq-main' },
          },
          runtime: 'Rule',
          kind: '变化上报',
          inputIds: ['pressure'],
          outputIds: ['pressure.reported'],
          inputs: 'pressure',
          outputs: 'pressure.reported',
          execution: '边端本地执行',
          validation: '已通过',
        },
      ],
    });

    const result = await fetchEdgeAlgorithms(
      'edge-dev',
      fetchMock as unknown as typeof fetch,
    );

    expect(fetchMock).toHaveBeenCalledWith('/api/edges/edge-dev/algorithms');
    expect(result[0].algorithmKind).toBe('ChangeReport');
    expect(result[0].inputIds).toEqual(['pressure']);
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

describe('saveEdgeAlgorithm', () => {
  it('sends an editable algorithm draft to the selected edge API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        edgeId: 'edge-dev',
        algorithmId: 'pump-anomaly-v1',
        version: '1.1.0',
        algorithmKind: 'WindowAggregate',
        dsl: {
          inputs: [{ alias: 'p', pointId: 'pressure' }],
          trigger: { type: 'window', everyMs: 60000 },
          steps: [
            {
              type: 'windowAggregate',
              source: 'p',
              functions: [{ function: 'avg', output: 'pressure_avg' }],
            },
          ],
          outputs: [{ name: 'pressure_avg', pointId: 'pressure.avg_1m' }],
          report: { mode: 'WindowResult', sink: 'velamq-main' },
        },
        runtime: 'Rule',
        kind: '窗口聚合',
        inputIds: ['pressure'],
        outputIds: ['pressure.avg_1m'],
        inputs: 'pressure',
        outputs: 'pressure.avg_1m',
        execution: '边端本地执行',
        validation: '已通过',
      }),
    });

    const result = await saveEdgeAlgorithm(
      'edge-dev',
      'pump-anomaly-v1',
      {
        version: '1.1.0',
        algorithmKind: 'WindowAggregate',
        dsl: {
          inputs: [{ alias: 'p', pointId: 'pressure' }],
          trigger: { type: 'window', everyMs: 60000 },
          steps: [
            {
              type: 'windowAggregate',
              source: 'p',
              functions: [{ function: 'avg', output: 'pressure_avg' }],
            },
          ],
          outputs: [{ name: 'pressure_avg', pointId: 'pressure.avg_1m' }],
          report: { mode: 'WindowResult', sink: 'velamq-main' },
        },
      },
      fetchMock as unknown as typeof fetch,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/edges/edge-dev/algorithms/pump-anomaly-v1',
      {
        body: JSON.stringify({
          version: '1.1.0',
          algorithmKind: 'WindowAggregate',
          dsl: {
            inputs: [{ alias: 'p', pointId: 'pressure' }],
            trigger: { type: 'window', everyMs: 60000 },
            steps: [
              {
                type: 'windowAggregate',
                source: 'p',
                functions: [{ function: 'avg', output: 'pressure_avg' }],
              },
            ],
            outputs: [{ name: 'pressure_avg', pointId: 'pressure.avg_1m' }],
            report: { mode: 'WindowResult', sink: 'velamq-main' },
          },
        }),
        headers: { 'content-type': 'application/json' },
        method: 'PUT',
      },
    );
    expect(result.kind).toBe('窗口聚合');
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

describe('saveEdgeProtocolConnection', () => {
  it('sends an editable protocol connection draft to the selected edge API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        edgeId: 'edge-dev',
        connectionId: 'modbus-line-a',
        protocolType: 'OpcUa',
        protocol: 'OPC UA',
        endpoint: 'opc.tcp://10.12.0.80:4840',
        status: '启用',
        policy: '1000ms timeout / 3 retry',
      }),
    });

    const result = await saveEdgeProtocolConnection(
      'edge-dev',
      'modbus-line-a',
      {
        endpoint: 'opc.tcp://10.12.0.80:4840',
        protocolType: 'OpcUa',
      },
      fetchMock as unknown as typeof fetch,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/edges/edge-dev/protocol-connections/modbus-line-a',
      {
        body: JSON.stringify({
          endpoint: 'opc.tcp://10.12.0.80:4840',
          protocolType: 'OpcUa',
        }),
        headers: { 'content-type': 'application/json' },
        method: 'PUT',
      },
    );
    expect(result.protocol).toBe('OPC UA');
  });
});

describe('createEdgeProtocolConnection', () => {
  it('creates a protocol connection draft for the selected edge API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        edgeId: 'edge-dev',
        connectionId: 'connection-draft-2',
        protocolType: 'ModbusTcp',
        protocol: 'Modbus TCP',
        endpoint: '10.12.0.30:502',
        status: '启用',
        policy: '1000ms timeout / 3 retry',
      }),
    });

    const result = await createEdgeProtocolConnection(
      'edge-dev',
      {
        endpoint: '10.12.0.30:502',
        protocolType: 'ModbusTcp',
      },
      fetchMock as unknown as typeof fetch,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/edges/edge-dev/protocol-connections',
      {
        body: JSON.stringify({
          endpoint: '10.12.0.30:502',
          protocolType: 'ModbusTcp',
        }),
        headers: { 'content-type': 'application/json' },
        method: 'POST',
      },
    );
    expect(result.connectionId).toBe('connection-draft-2');
  });
});

describe('draft creation clients', () => {
  it('creates point, collection task, algorithm, and device model drafts through APIs', async () => {
    const payloads: Record<string, unknown> = {
      '/api/edges/edge-dev/point-mappings': {
        edgeId: 'edge-dev',
        pointId: 'point-draft-3',
      },
      '/api/edges/edge-dev/collection-tasks': {
        edgeId: 'edge-dev',
        taskId: 'task-draft-2',
      },
      '/api/edges/edge-dev/algorithms': {
        edgeId: 'edge-dev',
        algorithmId: 'algorithm-draft-2',
      },
      '/api/device-models': {
        deviceType: 'device-model-draft-2',
      },
    };
    const fetchMock = vi.fn().mockImplementation((path: string) =>
      Promise.resolve({
        ok: true,
        json: async () => payloads[path],
      }),
    );

    await expect(
      createPointMappingDraft(
        'edge-dev',
        {
          addressKind: 'input_register',
          addressValue: '30001',
          connectionId: 'modbus-line-a',
          deviceId: 'pump-1',
          intervalMs: 2000,
          pointId: 'temperature',
          semanticId: 'pump.temperature',
          unit: 'C',
          valueType: 'float32',
        },
        fetchMock as unknown as typeof fetch,
      ),
    ).resolves.toMatchObject({ pointId: 'point-draft-3' });
    await expect(
      createCollectionTaskDraft(
        'edge-dev',
        {
          deviceId: 'pump-1',
          enabled: true,
          intervalMs: 1000,
          pointIds: ['pressure'],
          taskId: 'thermal-task',
        },
        fetchMock as unknown as typeof fetch,
      ),
    ).resolves.toMatchObject({ taskId: 'task-draft-2' });
    await expect(
      createAlgorithmDraft(
        'edge-dev',
        {
          algorithmId: 'thermal-rule',
          version: '1.0.0',
          algorithmKind: 'ThresholdRule',
          dsl: {
            inputs: [{ alias: 'p', pointId: 'pressure' }],
            trigger: { type: 'onSample' },
            steps: [
              {
                type: 'thresholdRule',
                source: 'p',
                operator: 'Gt',
                threshold: 0.2,
                event: {
                  code: 'THERMAL_ALERT',
                  severity: 'Warning',
                  message: '算法阈值告警',
                },
              },
            ],
            outputs: [{ name: 'alert', pointId: 'thermal.alert' }],
            report: { mode: 'EventOnly', sink: 'velamq-main' },
          },
        },
        fetchMock as unknown as typeof fetch,
      ),
    ).resolves.toMatchObject({ algorithmId: 'algorithm-draft-2' });
    await expect(
      createDeviceModelDraft(
        {
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
        },
        fetchMock as unknown as typeof fetch,
      ),
    ).resolves.toMatchObject({ deviceType: 'device-model-draft-2' });

    expect(fetchMock).toHaveBeenCalledWith('/api/edges/edge-dev/point-mappings', {
      body: JSON.stringify({
        addressKind: 'input_register',
        addressValue: '30001',
        connectionId: 'modbus-line-a',
        deviceId: 'pump-1',
        intervalMs: 2000,
        pointId: 'temperature',
        semanticId: 'pump.temperature',
        unit: 'C',
        valueType: 'float32',
      }),
      headers: { 'content-type': 'application/json' },
      method: 'POST',
    });
    expect(fetchMock).toHaveBeenCalledWith('/api/edges/edge-dev/collection-tasks', {
      body: JSON.stringify({
        deviceId: 'pump-1',
        enabled: true,
        intervalMs: 1000,
        pointIds: ['pressure'],
        taskId: 'thermal-task',
      }),
      headers: { 'content-type': 'application/json' },
      method: 'POST',
    });
    expect(fetchMock).toHaveBeenCalledWith('/api/edges/edge-dev/algorithms', {
      body: JSON.stringify({
        algorithmId: 'thermal-rule',
        version: '1.0.0',
        algorithmKind: 'ThresholdRule',
        dsl: {
          inputs: [{ alias: 'p', pointId: 'pressure' }],
          trigger: { type: 'onSample' },
          steps: [
            {
              type: 'thresholdRule',
              source: 'p',
              operator: 'Gt',
              threshold: 0.2,
              event: {
                code: 'THERMAL_ALERT',
                severity: 'Warning',
                message: '算法阈值告警',
              },
            },
          ],
          outputs: [{ name: 'alert', pointId: 'thermal.alert' }],
          report: { mode: 'EventOnly', sink: 'velamq-main' },
        },
      }),
      headers: { 'content-type': 'application/json' },
      method: 'POST',
    });
    expect(fetchMock).toHaveBeenCalledWith('/api/device-models', {
      body: JSON.stringify({
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
      }),
      headers: { 'content-type': 'application/json' },
      method: 'POST',
    });
  });
});

describe('device model save client', () => {
  it('saves edited device model definitions through the API', async () => {
    const request = {
      version: 'v2',
      telemetry: [
        {
          description: '泵体温度',
          range: '0-120',
          telemetryId: 'temperature',
          unit: 'C',
          valueType: 'float32',
        },
      ],
    };
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        commandCount: 0,
        deviceType: 'pump',
        eventCount: 0,
        telemetry: [],
        version: 'v2',
      }),
    });

    await expect(
      saveDeviceModel('pump', request, fetchMock as unknown as typeof fetch),
    ).resolves.toMatchObject({ deviceType: 'pump', version: 'v2' });
    expect(fetchMock).toHaveBeenCalledWith('/api/device-models/pump', {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    });
  });
});

describe('management action clients', () => {
  it('runs validation, release diff, and agent actions through APIs', async () => {
    const payloads: Record<string, unknown> = {
      '/api/edges/edge-dev/config/validate': {
        action: 'validate_config',
        status: '已通过',
      },
      '/api/edges/edge-dev/releases/diff': {
        action: 'release_diff',
        message: '配置差异摘要已生成',
      },
      '/api/agent/safety-check': {
        action: 'agent_safety_check',
        status: '已通过',
      },
      '/api/agent/suggestions': {
        action: 'agent_generate_suggestions',
        suggestions: [{ title: '点位补全' }],
      },
    };
    const fetchMock = vi.fn().mockImplementation((path: string) =>
      Promise.resolve({
        ok: true,
        json: async () => payloads[path],
      }),
    );

    await expect(
      runConfigValidation('edge-dev', fetchMock as unknown as typeof fetch),
    ).resolves.toMatchObject({ action: 'validate_config' });
    await expect(
      runReleaseDiff('edge-dev', fetchMock as unknown as typeof fetch),
    ).resolves.toMatchObject({ action: 'release_diff' });
    await expect(
      runAgentSafetyCheck(fetchMock as unknown as typeof fetch),
    ).resolves.toMatchObject({ action: 'agent_safety_check' });
    await expect(
      generateAgentSuggestions(fetchMock as unknown as typeof fetch),
    ).resolves.toMatchObject({ action: 'agent_generate_suggestions' });

    expect(fetchMock).toHaveBeenCalledWith('/api/edges/edge-dev/config/validate', {
      method: 'POST',
    });
    expect(fetchMock).toHaveBeenCalledWith('/api/edges/edge-dev/releases/diff', {
      method: 'POST',
    });
    expect(fetchMock).toHaveBeenCalledWith('/api/agent/safety-check', {
      method: 'POST',
    });
    expect(fetchMock).toHaveBeenCalledWith('/api/agent/suggestions', {
      method: 'POST',
    });
  });
});

describe('edge node lifecycle actions', () => {
  it('registers an edge node draft through the API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        edgeId: 'edge-draft-2',
        displayName: '一号产线边端',
        site: '制造/一号线',
        runtimeId: '-',
        status: '未上报',
        resources: '-',
        heartbeat: '-',
        capabilities: ['registration:draft'],
      }),
    });

    const result = await createEdgeNode(
      {
        displayName: '一号产线边端',
        site: '制造/一号线',
      },
      fetchMock as unknown as typeof fetch,
    );

    expect(fetchMock).toHaveBeenCalledWith('/api/edge-nodes', {
      body: JSON.stringify({
        displayName: '一号产线边端',
        site: '制造/一号线',
      }),
      headers: { 'content-type': 'application/json' },
      method: 'POST',
    });
    expect(result.edgeId).toBe('edge-draft-2');
  });

  it('rotates edge credentials through the API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        action: 'rotate_credentials',
        credentialVersion: 'credential-v2',
        edgeId: 'edge-dev',
        message: '凭证已轮换',
      }),
    });

    const result = await rotateEdgeCredentials(
      'edge-dev',
      fetchMock as unknown as typeof fetch,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/edges/edge-dev/credentials/rotate',
      {
        method: 'POST',
      },
    );
    expect(result.credentialVersion).toBe('credential-v2');
  });

  it('enables maintenance mode through the API', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        action: 'enable_maintenance',
        edgeId: 'edge-dev',
        message: '维护模式已启用',
        status: '维护中',
      }),
    });

    const result = await enableEdgeMaintenanceMode(
      'edge-dev',
      fetchMock as unknown as typeof fetch,
    );

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/edges/edge-dev/maintenance-mode',
      {
        method: 'POST',
      },
    );
    expect(result.status).toBe('维护中');
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
