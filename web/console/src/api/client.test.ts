import { describe, expect, it, vi } from 'vitest';

import {
  fetchPointMappings,
  fetchReleaseList,
  fetchSummary,
  publishLatestRelease,
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

describe('publishLatestRelease', () => {
  it('publishes the latest cloud draft and returns apply results', async () => {
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
            reportedVersion: '2026.06.26-002',
            result: '已应用',
            heartbeat: '18 秒前',
          },
        ],
      }),
    });

    const result = await publishLatestRelease(fetchMock as unknown as typeof fetch);

    expect(fetchMock).toHaveBeenCalledWith('/api/releases/publish', {
      method: 'POST',
    });
    expect(result.draftVersion).toBe('2026.06.26-002');
    expect(result.applyResults[0].result).toBe('已应用');
  });
});
