import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { CommandFlowConfig } from '../api/types';
import { ProductCommandFlowsEditor } from './ProductCommandFlowsEditor';

const writablePoints = [
  { access: 'read' as const, pointId: 'temperature', semanticId: 'pump.temperature' },
  { access: 'read_write' as const, pointId: 'running', semanticId: 'pump.running' },
  { access: 'write' as const, pointId: 'reset', semanticId: 'pump.reset' },
];

describe('ProductCommandFlowsEditor', () => {
  it('disables command orchestration when the product has no writable points', () => {
    render(
      <ProductCommandFlowsEditor
        flows={[]}
        mqttConnectionId="velamq-main"
        onChange={vi.fn()}
        points={[writablePoints[0]]}
      />,
    );

    expect(screen.getByRole('button', { name: '新建指令流程' })).toBeDisabled();
    expect(screen.getByRole('status')).toHaveTextContent('当前产品没有可写点位');
  });

  it('builds independent write branches for read-write and write-only points', () => {
    const onChange = vi.fn();
    render(
      <ProductCommandFlowsEditor
        flows={[]}
        mqttConnectionId="velamq-main"
        onChange={onChange}
        points={writablePoints}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '新建指令流程' }));
    const dialog = screen.getByRole('dialog', { name: '新建指令流程' });
    expect(within(dialog).queryByLabelText('选择写入点位 temperature')).not.toBeInTheDocument();
    fireEvent.click(within(dialog).getByLabelText('选择写入点位 running'));
    fireEvent.click(within(dialog).getByLabelText('选择写入点位 reset'));
    fireEvent.change(within(dialog).getByLabelText('消息字段路径 running'), {
      target: { value: 'payload.control.running' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存流程' }));

    expect(onChange).toHaveBeenCalledTimes(1);
    const [flow] = onChange.mock.calls[0][0] as CommandFlowConfig[];
    expect(flow.mqtt_connection_id).toBe('velamq-main');
    expect(flow.nodes.map((node) => node.kind)).toEqual([
      'mqtt_input',
      'json_parse',
      'safety_gate',
      'point_write',
      'point_write',
      'mqtt_reply',
    ]);

    const writeNodes = flow.nodes.filter((node) => node.kind === 'point_write');
    expect(writeNodes.map((node) => node.ref_id)).toEqual(['running', 'reset']);
    expect(writeNodes[0].params).toMatchObject({
      value_path: 'payload.control.running',
      verification: 'readback',
    });
    expect(writeNodes[1].params).toMatchObject({
      value_path: 'values.reset',
      verification: 'response',
    });
    expect(flow.edges).toEqual(expect.arrayContaining([
      expect.objectContaining({ from: 'safety', to: 'write-running' }),
      expect.objectContaining({ from: 'safety', to: 'write-reset' }),
      expect.objectContaining({ from: 'write-running', to: 'reply' }),
      expect.objectContaining({ from: 'write-reset', to: 'reply' }),
    ]));
  });

  it('preserves custom value paths when editing an existing command flow', () => {
    const onChange = vi.fn();
    const flow = commandFlowFixture();
    render(
      <ProductCommandFlowsEditor
        flows={[flow]}
        mqttConnectionId="velamq-main"
        onChange={onChange}
        points={writablePoints}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: `修改指令流程 ${flow.name}` }));
    const dialog = screen.getByRole('dialog', { name: '修改指令流程' });
    expect(within(dialog).getByLabelText('消息字段路径 running')).toHaveValue('value');
    fireEvent.change(within(dialog).getByLabelText('消息字段路径 running'), {
      target: { value: 'payload.running' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存流程' }));

    const [updated] = onChange.mock.calls[0][0] as CommandFlowConfig[];
    expect(updated.nodes.find((node) => node.kind === 'point_write')?.params.value_path).toBe('payload.running');
  });

  it('duplicates and deletes complete command flows', () => {
    const onChange = vi.fn();
    const flow = commandFlowFixture();
    const { rerender } = render(
      <ProductCommandFlowsEditor
        flows={[flow]}
        mqttConnectionId="velamq-main"
        onChange={onChange}
        points={writablePoints}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: `复制指令流程 ${flow.name}` }));
    const duplicated = onChange.mock.calls[0][0] as CommandFlowConfig[];
    expect(duplicated).toHaveLength(2);
    expect(duplicated[1].flow_id).not.toBe(flow.flow_id);
    expect(duplicated[1].nodes).not.toBe(flow.nodes);

    rerender(
      <ProductCommandFlowsEditor
        flows={[flow]}
        mqttConnectionId="velamq-main"
        onChange={onChange}
        points={writablePoints}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: `删除指令流程 ${flow.name}` }));
    const dialog = screen.getByRole('dialog', { name: '删除指令流程' });
    fireEvent.click(within(dialog).getByRole('button', { name: '确认删除' }));
    expect(onChange).toHaveBeenLastCalledWith([]);
  });
});

function commandFlowFixture(): CommandFlowConfig {
  return {
    edges: [
      { edge_id: 'input-write', from: 'input', to: 'write' },
      { edge_id: 'write-reply', from: 'write', to: 'reply' },
    ],
    enabled: true,
    flow_id: 'pump-control',
    mqtt_connection_id: 'velamq-main',
    name: '泵控制',
    nodes: [
      { kind: 'mqtt_input', label: '输入', node_id: 'input', params: {}, x: 0, y: 0 },
      { kind: 'point_write', label: '写运行状态', node_id: 'write', params: { value_path: 'value' }, ref_id: 'running', x: 200, y: 0 },
      { kind: 'mqtt_reply', label: '回执', node_id: 'reply', params: {}, x: 400, y: 0 },
    ],
    qos: 1,
    reply_topic_template: 'factory/{edge_id}/reply/{command_id}',
    subscribe_topic: 'factory/{edge_id}/command',
  };
}
