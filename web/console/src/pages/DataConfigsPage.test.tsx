import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { DataConfigsPage } from './DataConfigsPage';

describe('DataConfigsPage', () => {
  const existingConfig = {
    edgeId: 'edge-dev',
    configId: 'pump_status',
    name: '泵状态上报',
    enabled: true,
    deviceId: 'pump-1',
    protocolConnectionId: 'modbus-line-a',
    collection: { periodMs: 1000, retryCount: 2, timeoutMs: 800 },
    points: [
      {
        addressKind: 'holding_register',
        addressValue: '40001',
        jsonField: 'pressure',
        pointId: 'pressure',
        semanticId: 'pump.pressure',
        unit: 'MPa',
        valueType: 'float32',
      },
    ],
    algorithmIds: [],
    publish: {
      payload: { includeQuality: true, mode: 'object', timestampField: 'ts' },
      qos: 1,
      sinkId: 'velamq-main',
      topicTemplate: 'factory/{edge_id}/{device_id}/status',
    },
    visualGraph: { edges: [], nodes: [] },
  } as const;

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
        pointMappings={[
          {
            address: 'holding_register:40003',
            edgeId: 'edge-dev',
            pointId: 'flow_rate',
            semanticTelemetry: 'pump.flow_rate',
            unit: 'm3/h',
            valueType: 'float32',
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
    expect(within(dialog).getByLabelText('已选数据流资源')).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: /flow_rate/ }));
    expect(within(dialog).getByText('pump.flow_rate')).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: '移除点位 pressure' }));
    fireEvent.click(within(dialog).getByRole('button', { name: /pressure-change-report/ }));
    fireEvent.click(within(dialog).getByRole('button', { name: '下一步' }));

    fireEvent.change(within(dialog).getByLabelText('MQTT Topic'), {
      target: { value: 'factory/{edge_id}/{device_id}/status' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '下一步' }));

    expect(within(dialog).getByLabelText('JSON 预览')).toHaveValue();
    expect(
      (within(dialog).getByLabelText('JSON 预览') as HTMLTextAreaElement).value,
    ).toContain('flow_rate');
    expect(
      (within(dialog).getByLabelText('JSON 预览') as HTMLTextAreaElement).value,
    ).not.toContain('"pressure"');
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

  it('blocks save and explains missing publish fields before calling the API', () => {
    const onSave = vi.fn();

    render(
      <DataConfigsPage
        configs={[]}
        mqttUplink={{ sinkId: 'velamq-main', qos: 1 } as any}
        onSaveConfig={onSave}
        protocolConnections={[
          { connectionId: 'modbus-line-a', protocol: 'Modbus RTU' } as any,
        ]}
        selectedEdgeId="edge-dev"
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '新建数据上报' }));
    const dialog = screen.getByRole('dialog', { name: '新建数据上报' });
    fireEvent.click(within(dialog).getByRole('button', { name: '3. 上报规则' }));
    fireEvent.change(within(dialog).getByLabelText('MQTT Topic'), {
      target: { value: '' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '4. JSON 预览' }));
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    expect(within(dialog).getByRole('alert')).toHaveTextContent('MQTT Topic 不能为空');
    expect(onSave).not.toHaveBeenCalled();
  });

  it('duplicates and toggles an existing data config through the save API', () => {
    const onSave = vi.fn();

    render(
      <DataConfigsPage
        configs={[existingConfig as any]}
        onSaveConfig={onSave}
        selectedEdgeId="edge-dev"
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '复制' }));
    expect(onSave).toHaveBeenCalledWith(
      'edge-dev',
      null,
      expect.objectContaining({
        configId: 'pump_status_copy',
        name: '泵状态上报 副本',
      }),
    );

    fireEvent.click(screen.getByRole('button', { name: '暂停' }));
    expect(onSave).toHaveBeenCalledWith(
      'edge-dev',
      'pump_status',
      expect.objectContaining({
        configId: 'pump_status',
        enabled: false,
      }),
    );
  });

  it('filters data configs by keyword and enabled state', () => {
    const pausedConfig = {
      ...existingConfig,
      configId: 'pump_energy',
      enabled: false,
      name: '泵能耗上报',
      publish: {
        ...existingConfig.publish,
        topicTemplate: 'factory/{edge_id}/{device_id}/energy',
      },
    };

    render(
      <DataConfigsPage
        configs={[existingConfig as any, pausedConfig as any]}
        selectedEdgeId="edge-dev"
      />,
    );

    fireEvent.change(screen.getByLabelText('搜索数据上报'), {
      target: { value: 'energy' },
    });

    expect(screen.getByText('pump_energy')).toBeInTheDocument();
    expect(screen.queryByText('pump_status')).not.toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('状态筛选'), {
      target: { value: 'disabled' },
    });

    expect(screen.getByText('pump_energy')).toBeInTheDocument();
    expect(screen.getByText('已筛选 1 / 2 套配置')).toBeInTheDocument();
  });

  it('searches and bulk manages visual point resources', () => {
    render(
      <DataConfigsPage
        configs={[]}
        pointMappings={[
          {
            address: 'holding_register:40003',
            edgeId: 'edge-dev',
            pointId: 'flow_rate',
            semanticTelemetry: 'pump.flow_rate',
            unit: 'm3/h',
            valueType: 'float32',
          } as any,
          {
            address: 'coil:1',
            edgeId: 'edge-dev',
            pointId: 'running',
            semanticTelemetry: 'pump.running',
            unit: '-',
            valueType: 'bool',
          } as any,
        ]}
        selectedEdgeId="edge-dev"
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '新建数据上报' }));
    const dialog = screen.getByRole('dialog', { name: '新建数据上报' });
    fireEvent.click(within(dialog).getByRole('button', { name: '2. 可视化编排' }));

    fireEvent.change(within(dialog).getByLabelText('搜索点位资源'), {
      target: { value: 'running' },
    });

    expect(within(dialog).queryByRole('button', { name: /flow_rate/ })).not.toBeInTheDocument();
    expect(within(dialog).getByRole('button', { name: /running/ })).toBeInTheDocument();

    fireEvent.click(within(dialog).getByRole('button', { name: '加入筛选点位' }));

    expect(within(dialog).getByText('2 个点位')).toBeInTheDocument();

    fireEvent.click(within(dialog).getByRole('button', { name: '清空点位' }));

    expect(within(dialog).getByText('0 个点位')).toBeInTheDocument();
  });

  it('blocks save when multiple points map to the same JSON field', () => {
    const onSave = vi.fn();
    const duplicateConfig = {
      ...existingConfig,
      points: [
        existingConfig.points[0],
        {
          ...existingConfig.points[0],
          addressValue: '40002',
          pointId: 'pressure_b',
        },
      ],
    };

    render(
      <DataConfigsPage
        configs={[duplicateConfig as any]}
        onSaveConfig={onSave}
        selectedEdgeId="edge-dev"
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'pump_status' }));
    const dialog = screen.getByRole('dialog', { name: '编辑数据上报' });
    fireEvent.click(within(dialog).getByRole('button', { name: '4. JSON 预览' }));
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));

    expect(within(dialog).getByRole('alert')).toHaveTextContent('JSON 字段 pressure 重复');
    expect(onSave).not.toHaveBeenCalled();
  });
});
