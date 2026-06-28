import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { DeviceModelsPage } from './DeviceModelsPage';

describe('DeviceModelsPage', () => {
  it('opens a device model form and creates a model from user input', async () => {
    const onCreateDeviceModel = vi.fn().mockResolvedValue({
      deviceType: 'meter',
      version: 'v2',
      telemetry: [
        {
          description: 'A 相电压',
          name: 'voltage_a',
          range: '0-500',
          telemetryId: 'voltage_a',
          unit: 'V',
          valueType: 'float32',
        },
      ],
    });

    render(<DeviceModelsPage onCreateDeviceModel={onCreateDeviceModel} />);

    fireEvent.click(screen.getByRole('button', { name: '新建设备模型' }));
    expect(screen.getByRole('dialog', { name: '新建设备模型' })).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('设备类型'), {
      target: { value: 'meter' },
    });
    fireEvent.change(screen.getByLabelText('模型版本'), {
      target: { value: 'v2' },
    });
    fireEvent.change(screen.getByLabelText('遥测 ID'), {
      target: { value: 'voltage_a' },
    });
    fireEvent.change(screen.getByLabelText('数据类型'), {
      target: { value: 'float32' },
    });
    fireEvent.change(screen.getByLabelText('单位'), {
      target: { value: 'V' },
    });
    fireEvent.change(screen.getByLabelText('范围'), {
      target: { value: '0-500' },
    });
    fireEvent.change(screen.getByLabelText('说明'), {
      target: { value: 'A 相电压' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存设备模型' }));

    await waitFor(() => {
      expect(onCreateDeviceModel).toHaveBeenCalledWith({
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
      });
    });
    expect(await screen.findByText('已创建设备模型 meter@v2')).toBeInTheDocument();
  });
});
