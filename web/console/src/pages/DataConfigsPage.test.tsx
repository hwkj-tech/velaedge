import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { DataConfigsPage } from './DataConfigsPage';

describe('DataConfigsPage', () => {
  it('opens a step dialog and saves a complete data config', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);

    render(
      <DataConfigsPage
        configs={[]}
        edges={[{ edgeId: 'edge-dev', displayName: '研发实验室边端' } as any]}
        mqttUplink={{ sinkId: 'velamq-main', qos: 1 } as any}
        onSaveConfig={onSave}
        algorithms={[
          {
            algorithmId: 'pressure-change-report',
            algorithmKind: 'ChangeReport',
            edgeId: 'edge-dev',
            inputIds: ['pressure'],
            outputIds: ['pressure_changed'],
          } as any,
        ]}
        protocolConnections={[
          { connectionId: 'modbus-line-a', protocol: 'Modbus RTU' } as any,
        ]}
        selectedEdgeId="edge-dev"
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '新建数据上报' }));
    const dialog = screen.getByRole('dialog', { name: '新建数据上报' });

    fireEvent.change(within(dialog).getByLabelText('配置 ID'), {
      target: { value: 'pump_status' },
    });
    fireEvent.change(within(dialog).getByLabelText('配置名称'), {
      target: { value: '泵运行状态上报' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '下一步' }));

    expect(within(dialog).getByText('pressure-change-report')).toBeInTheDocument();
    expect(within(dialog).getByLabelText('数据上报画布')).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: /pressure-change-report/ }));
    fireEvent.click(within(dialog).getByRole('button', { name: '下一步' }));

    fireEvent.change(within(dialog).getByLabelText('MQTT Topic'), {
      target: { value: 'factory/{edge_id}/{device_id}/status' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '下一步' }));

    expect(within(dialog).getByLabelText('JSON 预览')).toHaveValue();
    expect(
      (within(dialog).getByLabelText('JSON 预览') as HTMLTextAreaElement).value,
    ).toContain('pressure');
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    expect(onSave).toHaveBeenCalledWith(
      'edge-dev',
      null,
      expect.objectContaining({
        collection: expect.objectContaining({ periodMs: 1000 }),
        configId: 'pump_status',
        algorithmIds: ['pressure-change-report'],
        visualGraph: expect.objectContaining({
          nodes: expect.arrayContaining([
            expect.objectContaining({ kind: 'algorithm', refId: 'pressure-change-report' }),
            expect.objectContaining({ kind: 'json' }),
            expect.objectContaining({ kind: 'mqtt' }),
          ]),
        }),
        publish: expect.objectContaining({
          topicTemplate: 'factory/{edge_id}/{device_id}/status',
        }),
      }),
    );
  });
});
