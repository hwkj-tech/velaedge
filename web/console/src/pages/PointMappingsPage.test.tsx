import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { PointMappingsPage } from './PointMappingsPage';

const pointFixtures = [
  {
    edgeId: 'edge-dev', pointId: 'pressure', pointName: '泵出口压力', deviceId: 'pump-1',
    deviceModel: 'pump@v1', semanticTelemetry: 'pump.pressure', protocol: 'Modbus TCP',
    connection: 'modbus-line-a', address: 'holding_register:40001', valueType: 'float32',
    readWrite: 'read', unit: 'MPa', scale: '0.1', interval: '1000ms', range: '0-20',
    qualityRule: 'timeout->bad', status: '启用',
  },
  {
    edgeId: 'edge-dev', pointId: 'running', pointName: '运行状态', deviceId: 'pump-1',
    deviceModel: 'pump@v1', semanticTelemetry: 'pump.running', protocol: 'Modbus TCP',
    connection: 'modbus-line-a', address: 'coil:00001', valueType: 'bool',
    readWrite: 'read', unit: '-', scale: '1', interval: '1000ms', range: '-',
    qualityRule: 'stale->bad', status: '启用',
  },
] as any;

describe('PointMappingsPage', () => {
  it('shows point sets as the primary management unit', () => {
    render(<PointMappingsPage points={pointFixtures} />);

    expect(screen.getByRole('heading', { name: '点位集管理' })).toBeInTheDocument();
    expect(screen.getByText('点位集列表')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '查看点位集 pump-1 / modbus-line-a' })).toBeInTheDocument();
    expect(screen.getByText('pressure, running')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '新建点位' })).not.toBeInTheDocument();
  });

  it('opens a point set detail drawer and edits a point inside the set', async () => {
    const onSavePoint = vi.fn().mockResolvedValue(undefined);
    render(<PointMappingsPage points={pointFixtures} selectedEdgeId="edge-dev" onSavePoint={onSavePoint} />);

    fireEvent.click(screen.getByRole('button', { name: '修改点位集 pump-1 / modbus-line-a' }));

    const dialog = screen.getByRole('dialog', { name: '点位集 pump-1 / modbus-line-a' });
    expect(within(dialog).getByText('集合内点位')).toBeInTheDocument();
    expect(within(dialog).getByRole('button', { name: '选择点位 pressure' })).toBeInTheDocument();
    expect(within(dialog).getByRole('button', { name: '选择点位 running' })).toBeInTheDocument();

    fireEvent.change(within(dialog).getByLabelText('地址值'), {
      target: { value: '40002' },
    });
    fireEvent.change(within(dialog).getByLabelText('采集周期(ms)'), {
      target: { value: '2000' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(onSavePoint).toHaveBeenCalledWith('edge-dev', 'pressure', {
        addressKind: 'holding_register',
        addressValue: '40002',
        intervalMs: 2000,
        readWrite: 'read',
        unit: 'MPa',
      });
    });
  });

  it('closes point set dialogs with Escape', () => {
    render(<PointMappingsPage points={pointFixtures} selectedEdgeId="edge-dev" />);

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    expect(screen.getByRole('dialog', { name: '新建点位集' })).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('dialog', { name: '新建点位集' })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '修改点位集 pump-1 / modbus-line-a' }));
    expect(screen.getByRole('dialog', { name: '点位集 pump-1 / modbus-line-a' })).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('dialog', { name: '点位集 pump-1 / modbus-line-a' })).not.toBeInTheDocument();
  });

  it('switches selected point inside a point set before saving', async () => {
    const onSavePoint = vi.fn().mockResolvedValue(undefined);
    render(<PointMappingsPage points={pointFixtures} selectedEdgeId="edge-dev" onSavePoint={onSavePoint} />);

    fireEvent.click(screen.getByRole('button', { name: '修改点位集 pump-1 / modbus-line-a' }));
    const dialog = screen.getByRole('dialog', { name: '点位集 pump-1 / modbus-line-a' });
    fireEvent.click(within(dialog).getByRole('button', { name: '选择点位 running' }));
    fireEvent.change(within(dialog).getByLabelText('地址值'), {
      target: { value: '00002' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(onSavePoint).toHaveBeenCalledWith('edge-dev', 'running', {
        addressKind: 'coil',
        addressValue: '00002',
        intervalMs: 1000,
        readWrite: 'read',
        unit: '-',
      });
    });
  });

  it('persists explicit writable access for command orchestration', async () => {
    const onSavePoint = vi.fn().mockResolvedValue(undefined);
    render(<PointMappingsPage points={pointFixtures} selectedEdgeId="edge-dev" onSavePoint={onSavePoint} />);

    fireEvent.click(screen.getByRole('button', { name: '修改点位集 pump-1 / modbus-line-a' }));
    const dialog = screen.getByRole('dialog', { name: '点位集 pump-1 / modbus-line-a' });
    fireEvent.change(within(dialog).getByLabelText('访问权限'), {
      target: { value: 'read_write' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(onSavePoint).toHaveBeenCalledWith('edge-dev', 'pressure', expect.objectContaining({
        readWrite: 'read_write',
      }));
    });
  });

  it('creates a point set by submitting multiple points', async () => {
    const onCreatePoint = vi.fn()
      .mockResolvedValueOnce({ pointId: 'temperature' })
      .mockResolvedValueOnce({ pointId: 'running' });

    render(<PointMappingsPage selectedEdgeId="edge-dev" onCreatePoint={onCreatePoint} />);

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    const dialog = screen.getByRole('dialog', { name: '新建点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位集名称'), {
      target: { value: 'pump-basic-set' },
    });
    fireEvent.change(within(dialog).getByLabelText('新建设备 ID'), {
      target: { value: 'pump-1' },
    });
    fireEvent.change(within(dialog).getByLabelText('新建连接实例'), {
      target: { value: 'modbus-line-a' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位 1 Point ID'), {
      target: { value: 'temperature' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位 1 语义遥测'), {
      target: { value: 'pump.temperature' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位 1 地址类型'), {
      target: { value: 'input_register' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位 1 地址值'), {
      target: { value: '30001' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位 1 采集周期(ms)'), {
      target: { value: '5000' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位 1 单位'), {
      target: { value: 'C' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位 2 Point ID'), {
      target: { value: 'running' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位 2 语义遥测'), {
      target: { value: 'pump.running' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位 2 地址值'), {
      target: { value: '00001' },
    });
    fireEvent.change(within(dialog).getByLabelText('点位 2 采集周期(ms)'), {
      target: { value: '2000' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(onCreatePoint).toHaveBeenCalledTimes(2);
      expect(onCreatePoint).toHaveBeenCalledWith('edge-dev', {
        addressKind: 'input_register',
        addressValue: '30001',
        connectionId: 'modbus-line-a',
        deviceId: 'pump-1',
        intervalMs: 5000,
        pointId: 'temperature',
        semanticId: 'pump.temperature',
        unit: 'C',
        valueType: 'float32',
      });
      expect(onCreatePoint).toHaveBeenCalledWith('edge-dev', {
        addressKind: 'coil',
        addressValue: '00001',
        connectionId: 'modbus-line-a',
        deviceId: 'pump-1',
        intervalMs: 2000,
        pointId: 'running',
        semanticId: 'pump.running',
        unit: '-',
        valueType: 'bool',
      });
    });
    expect(await screen.findByText('已创建点位集 pump-basic-set，包含 2 个点位')).toBeInTheDocument();
  });

  it('runs point set toolbar actions through handlers', async () => {
    const onImportPoints = vi.fn().mockResolvedValue({ message: '批量导入已完成' });
    const onValidateDraft = vi.fn().mockResolvedValue({ status: '已通过' });

    render(
      <PointMappingsPage
        selectedEdgeId="edge-dev"
        onImportPoints={onImportPoints}
        onValidateDraft={onValidateDraft}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '批量导入' }));
    await waitFor(() => {
      expect(onImportPoints).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findByText('批量导入已完成')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '校验配置' }));
    await waitFor(() => {
      expect(onValidateDraft).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findByText('点位配置校验 已通过')).toBeInTheDocument();
  });

  it('hides edge selection context in list mode', () => {
    render(<PointMappingsPage mode="list" selectedEdgeId="edge-dev" />);

    expect(screen.getByText('点位集列表')).toBeInTheDocument();
    expect(screen.queryByLabelText('查看边端')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('配置边端')).not.toBeInTheDocument();
  });
});
