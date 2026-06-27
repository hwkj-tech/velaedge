import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { PointMappingsPage } from './PointMappingsPage';

describe('PointMappingsPage', () => {
  it('shows point table and editor drawer fields', () => {
    render(<PointMappingsPage />);

    expect(screen.getByText('点位配置表')).toBeInTheDocument();
    expect(screen.getByText('pressure')).toBeInTheDocument();
    expect(screen.getByText('holding_register:40001')).toBeInTheDocument();
    expect(screen.getByText('编辑点位 pressure')).toBeInTheDocument();
    expect(screen.getByText('采集周期')).toBeInTheDocument();
  });

  it('saves edited point mapping drafts from the editor drawer', async () => {
    const onSavePoint = vi.fn().mockResolvedValue(undefined);

    render(<PointMappingsPage selectedEdgeId="edge-dev" onSavePoint={onSavePoint} />);

    fireEvent.change(screen.getByLabelText('地址值'), {
      target: { value: '40002' },
    });
    fireEvent.change(screen.getByLabelText('采集周期(ms)'), {
      target: { value: '2000' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存草稿' }));

    await waitFor(() => {
      expect(onSavePoint).toHaveBeenCalledWith(
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
    expect(screen.getByText('草稿已保存')).toBeInTheDocument();
  });

  it('switches the editor to the selected point row before saving', async () => {
    const onSavePoint = vi.fn().mockResolvedValue(undefined);

    render(<PointMappingsPage selectedEdgeId="edge-dev" onSavePoint={onSavePoint} />);

    fireEvent.click(screen.getByRole('button', { name: '选择点位 running' }));
    expect(screen.getByText('编辑点位 running')).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('地址值'), {
      target: { value: '00002' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存草稿' }));

    await waitFor(() => {
      expect(onSavePoint).toHaveBeenCalledWith(
        'edge-dev',
        'running',
        {
          addressKind: 'coil',
          addressValue: '00002',
          intervalMs: 1000,
          unit: '-',
        },
      );
    });
  });

  it('switches the active edge before editing point mappings', async () => {
    const onSelectEdge = vi.fn().mockResolvedValue(undefined);

    render(
      <PointMappingsPage
        edges={[
          {
            edgeId: 'edge-dev',
            displayName: '研发实验室边端',
            site: '研发/实验室',
            runtimeId: 'runtime-dev',
            status: '健康',
            resources: '18% / 42% / 61%',
            heartbeat: '8 秒前',
            capabilities: ['protocol:modbus-tcp'],
          },
          {
            edgeId: 'edge-prod',
            displayName: '产线边端',
            site: '制造/一线',
            runtimeId: 'runtime-prod',
            status: '健康',
            resources: '22% / 48% / 66%',
            heartbeat: '6 秒前',
            capabilities: ['protocol:opcua'],
          },
        ]}
        selectedEdgeId="edge-dev"
        onSelectEdge={onSelectEdge}
      />,
    );

    fireEvent.change(screen.getByLabelText('配置边端'), {
      target: { value: 'edge-prod' },
    });

    await waitFor(() => {
      expect(onSelectEdge).toHaveBeenCalledWith('edge-prod');
    });
  });

  it('runs point toolbar actions through handlers', async () => {
    const onCreatePoint = vi.fn().mockResolvedValue({
      pointId: 'point-draft-3',
    });
    const onImportPoints = vi.fn().mockResolvedValue({
      message: '批量导入已完成',
    });
    const onValidateDraft = vi.fn().mockResolvedValue({
      status: '已通过',
    });

    render(
      <PointMappingsPage
        selectedEdgeId="edge-dev"
        onCreatePoint={onCreatePoint}
        onImportPoints={onImportPoints}
        onValidateDraft={onValidateDraft}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '批量导入' }));
    await waitFor(() => {
      expect(onImportPoints).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findByText('批量导入已完成')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '校验草稿' }));
    await waitFor(() => {
      expect(onValidateDraft).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findByText('点位草稿校验 已通过')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '新建点位' }));
    await waitFor(() => {
      expect(onCreatePoint).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findByText('已创建点位草稿 point-draft-3')).toBeInTheDocument();
  });
});
