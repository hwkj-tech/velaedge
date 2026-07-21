import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { PointSetResponse } from '../api/types';
import { PointSetsPage } from './PointSetsPage';

const pointSet: PointSetResponse = {
  createdAt: '2026-07-14T00:00:00Z',
  description: '泵站基础点位',
  name: '泵站基础点位',
  pointSetId: 'pump-standard-points',
  points: [
    {
      address: { kind: 'holding_register', value: '40001' },
      intervalMs: 1000,
      pointId: 'pressure',
      semanticId: 'pump.pressure',
      unit: 'MPa',
      valueType: 'float32',
    },
  ],
  projectId: 'demo-plant',
  protocol: 'ModbusRtu',
  updatedAt: '2026-07-14T00:00:00Z',
};

const projects = [{ name: '示例工厂', projectId: 'demo-plant' }];

describe('PointSetsPage', () => {
  it('renders persisted point sets as reusable catalog resources', () => {
    renderPage();

    expect(screen.getByRole('heading', { name: '点位集管理' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '查看点位集 泵站基础点位' })).toBeInTheDocument();
    expect(screen.getByText('示例工厂')).toBeInTheDocument();
    expect(screen.getByText('Modbus RTU')).toBeInTheDocument();
    expect(screen.getByText('1000ms')).toBeInTheDocument();
  });

  it('creates a point set as one resource with multiple points', async () => {
    const onCreate = vi.fn().mockResolvedValue(pointSet);
    renderPage({ onCreate, pointSets: [] });

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    const dialog = screen.getByRole('dialog', { name: '新建点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位集 ID'), { target: { value: 'meter-points' } });
    fireEvent.change(within(dialog).getByLabelText('点位集名称'), { target: { value: '电表点位' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 Point ID'), { target: { value: 'voltage_a' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 语义 ID'), { target: { value: 'meter.voltage_a' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 地址值'), { target: { value: '40001' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 采集周期(ms)'), { target: { value: '2000' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '添加点位' }));
    fireEvent.change(within(dialog).getByLabelText('点位 2 Point ID'), { target: { value: 'current_a' } });
    fireEvent.change(within(dialog).getByLabelText('点位 2 语义 ID'), { target: { value: 'meter.current_a' } });
    fireEvent.change(within(dialog).getByLabelText('点位 2 地址值'), { target: { value: '40003' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    const request = onCreate.mock.calls[0][0];
    expect(request.pointSetId).toBe('meter-points');
    expect(request.projectId).toBe('demo-plant');
    expect(request.points).toHaveLength(2);
    expect(request.points[0].intervalMs).toBe(2000);
  });

  it('saves the whole point set and supports Escape', async () => {
    const onSave = vi.fn().mockResolvedValue(pointSet);
    renderPage({ onSave });

    fireEvent.click(screen.getByRole('button', { name: '修改点位集 泵站基础点位' }));
    const dialog = screen.getByRole('dialog', { name: '编辑点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位 1 采集周期(ms)'), { target: { value: '5000' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(
      'pump-standard-points',
      expect.objectContaining({ points: [expect.objectContaining({ intervalMs: 5000 })] }),
    ));

    fireEvent.click(screen.getByRole('button', { name: '修改点位集 泵站基础点位' }));
    expect(screen.getByRole('dialog', { name: '编辑点位集' })).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('dialog', { name: '编辑点位集' })).not.toBeInTheDocument();
  });

  it('deletes a complete point set after confirmation', async () => {
    const onDelete = vi.fn().mockResolvedValue(undefined);
    renderPage({ onDelete });

    fireEvent.click(screen.getByRole('button', { name: '删除点位集 泵站基础点位' }));
    const dialog = screen.getByRole('dialog', { name: '删除点位集' });
    fireEvent.click(within(dialog).getByRole('button', { name: '确认删除' }));

    await waitFor(() => expect(onDelete).toHaveBeenCalledWith('pump-standard-points'));
  });

  it('builds a structured custom serial frame instead of requiring raw JSON', async () => {
    const onCreate = vi.fn().mockResolvedValue(pointSet);
    renderPage({ onCreate, pointSets: [] });

    fireEvent.click(screen.getByRole('button', { name: '新建点位集' }));
    const dialog = screen.getByRole('dialog', { name: '新建点位集' });
    fireEvent.change(within(dialog).getByLabelText('点位集 ID'), { target: { value: 'vendor-serial' } });
    fireEvent.change(within(dialog).getByLabelText('点位集名称'), { target: { value: '厂商串口点位' } });
    fireEvent.change(within(dialog).getByLabelText('协议'), { target: { value: 'CustomSerial' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 Point ID'), { target: { value: 'temperature' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 语义 ID'), { target: { value: 'sensor.temperature' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 请求帧 HEX'), { target: { value: '10 02' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 请求校验'), { target: { value: 'sum8' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 响应校验'), { target: { value: 'sum8' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 取值偏移'), { target: { value: '1' } });
    fireEvent.change(within(dialog).getByLabelText('点位 1 缩放'), { target: { value: '0.1' } });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledTimes(1));
    const request = onCreate.mock.calls[0][0];
    expect(request.points[0].address.kind).toBe('custom_serial_frame');
    expect(JSON.parse(request.points[0].address.value)).toMatchObject({
      requestChecksum: 'sum8',
      requestHex: '10 02',
      responseChecksum: 'sum8',
      scale: 0.1,
      valueEncoding: 'u16_be',
      valueOffset: 1,
    });
  });
});

function renderPage(overrides: Partial<Parameters<typeof PointSetsPage>[0]> = {}) {
  return render(
    <PointSetsPage
      onCreate={vi.fn().mockResolvedValue(pointSet)}
      onDelete={vi.fn().mockResolvedValue(undefined)}
      onSave={vi.fn().mockResolvedValue(pointSet)}
      pointSets={[pointSet]}
      projects={projects}
      {...overrides}
    />,
  );
}
