import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  fetchPointMappings,
  fetchReleaseList,
  fetchSummary,
  publishLatestRelease,
  savePointMapping,
} from './api/client';
import type { PointMappingResponse, ReleaseListResponse } from './api/types';
import App from './App';

vi.mock('./api/client', () => ({
  fetchPointMappings: vi.fn(),
  fetchReleaseList: vi.fn(),
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

describe('App cloud console write actions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(fetchSummary).mockResolvedValue({
      edge_count: 1,
      pending_release_count: 0,
    });
    vi.mocked(fetchPointMappings).mockResolvedValue([basePoint]);
    vi.mocked(fetchReleaseList).mockResolvedValue(initialReleaseList);
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
      expect(publishLatestRelease).toHaveBeenCalledOnce();
    });
    await waitFor(() => {
      expect(screen.getAllByText('2026.06.26-002').length).toBeGreaterThan(0);
    });
  });
});
