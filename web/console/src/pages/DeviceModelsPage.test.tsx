import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
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
    const dialog = screen.getByRole('dialog', { name: '新建设备模型' });
    expect(dialog).toBeInTheDocument();

    fireEvent.change(within(dialog).getByLabelText('设备类型'), {
      target: { value: 'meter' },
    });
    fireEvent.change(within(dialog).getByLabelText('模型版本'), {
      target: { value: 'v2' },
    });
    fireEvent.change(within(dialog).getByLabelText('遥测 ID'), {
      target: { value: 'voltage_a' },
    });
    fireEvent.change(within(dialog).getByLabelText('数据类型'), {
      target: { value: 'float32' },
    });
    fireEvent.change(within(dialog).getByLabelText('单位'), {
      target: { value: 'V' },
    });
    fireEvent.change(within(dialog).getByLabelText('范围'), {
      target: { value: '0-500' },
    });
    fireEvent.change(within(dialog).getByLabelText('说明'), {
      target: { value: 'A 相电压' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存设备模型' }));

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

  it('shows multiple models and saves edits for the selected model', async () => {
    const onSaveDeviceModel = vi.fn().mockResolvedValue({
      commandCount: 0,
      deviceType: 'meter',
      eventCount: 0,
      version: 'v3',
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

    render(
      <DeviceModelsPage
        deviceModels={[
          {
            commandCount: 1,
            deviceType: 'pump',
            eventCount: 1,
            version: 'v1',
            telemetry: [
              {
                description: '泵出口压力',
                name: 'pressure',
                range: '0-20',
                telemetryId: 'pressure',
                unit: 'MPa',
                valueType: 'float32',
              },
            ],
          },
          {
            commandCount: 0,
            deviceType: 'meter',
            eventCount: 0,
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
          },
        ]}
        onSaveDeviceModel={onSaveDeviceModel}
      />,
    );

    expect(screen.getByText('设备模型清单')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '选择设备模型 meter' }));

    expect(screen.getByText('编辑设备模型 meter')).toBeInTheDocument();
    const editor = screen.getByRole('dialog', {
      name: '编辑设备模型 meter',
    });
    fireEvent.change(within(editor).getByLabelText('模型版本'), {
      target: { value: 'v3' },
    });
    fireEvent.click(within(editor).getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(onSaveDeviceModel).toHaveBeenCalledWith('meter', {
        version: 'v3',
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
    expect(await screen.findByText('已保存')).toBeInTheDocument();
  });

  it('deletes the selected device model from the list action', async () => {
    const onDeleteDeviceModel = vi.fn().mockResolvedValue(undefined);

    render(
      <DeviceModelsPage
        deviceModels={[
          {
            commandCount: 0,
            deviceType: 'meter',
            eventCount: 0,
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
          },
        ]}
        onDeleteDeviceModel={onDeleteDeviceModel}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '删除设备模型 meter' }));

    await waitFor(() => {
      expect(onDeleteDeviceModel).toHaveBeenCalledWith('meter');
    });
    expect(await screen.findByText('已删除设备模型 meter')).toBeInTheDocument();
  });
});
