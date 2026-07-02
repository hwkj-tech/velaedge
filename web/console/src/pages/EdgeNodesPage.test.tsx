import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { EdgeNodeResponse } from '../api/types';
import { EdgeNodesPage } from './EdgeNodesPage';

const edges: EdgeNodeResponse[] = [
  {
    edgeId: 'edge-dev',
    displayName: '研发实验室边端',
    site: '研发/实验室',
    runtimeId: 'runtime-dev',
    status: '健康',
    resources: '18.5% / 42% / 61%',
    heartbeat: '8 秒前',
    capabilities: ['protocol:modbus-tcp'],
  },
];

describe('EdgeNodesPage', () => {
  it('exposes per-edge configuration and monitoring actions', () => {
    const onConfigureEdge = vi.fn();
    const onMonitorEdge = vi.fn();

    render(
      <EdgeNodesPage
        configSummaries={[
          {
            collectionTaskCount: 1,
            dataConfigCount: 1,
            edgeId: 'edge-dev',
            mqttSinkId: 'velamq-main',
            pointCount: 4,
            protocolCount: 2,
            releaseStatus: '已发布',
          },
        ]}
        edges={edges}
        onConfigureEdge={onConfigureEdge}
        onMonitorEdge={onMonitorEdge}
      />,
    );

    expect(screen.getByRole('columnheader', { name: '配置摘要' })).toBeInTheDocument();
    expect(screen.getByText('2 协议')).toBeInTheDocument();
    expect(screen.getByText('4 点位')).toBeInTheDocument();
    expect(screen.getByText('1 任务')).toBeInTheDocument();
    expect(screen.getByText('1 上报')).toBeInTheDocument();
    expect(screen.getByText('已发布')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '配置 edge-dev' }));
    expect(screen.getByRole('dialog', { name: '配置边端' })).toBeInTheDocument();
    expect(screen.getByText('配置完整度')).toBeInTheDocument();
    expect(screen.getByText('100%')).toBeInTheDocument();
    expect(screen.getByText('配置链路状态良好')).toBeInTheDocument();
    expect(screen.getByText('采集、处理、上报和发布链路基本闭环')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '打开配置总览' }));
    fireEvent.click(screen.getByRole('button', { name: '运行监控 edge-dev' }));

    expect(onConfigureEdge).toHaveBeenCalledWith('edge-dev', 'overview');
    expect(onMonitorEdge).toHaveBeenCalledWith('edge-dev');
  });

  it('opens a selected configuration section from the smart binding dialog', () => {
    const onConfigureEdge = vi.fn();

    render(
      <EdgeNodesPage
        configSummaries={[
          {
            collectionTaskCount: 1,
            dataConfigCount: 0,
            edgeId: 'edge-dev',
            mqttSinkId: 'velamq-main',
            pointCount: 4,
            protocolCount: 1,
            releaseStatus: '待发布',
          },
        ]}
        edges={edges}
        onConfigureEdge={onConfigureEdge}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '配置 edge-dev' }));
    expect(screen.getByText('建议创建数据上报配置')).toBeInTheDocument();
    expect(screen.getByText('67%')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '配置上报' }));

    expect(onConfigureEdge).toHaveBeenCalledWith('edge-dev', 'reports');
  });

  it('opens and saves per-edge mqtt configuration', async () => {
    const onSaveMqttUplink = vi.fn().mockResolvedValue({
      batchSize: 100,
      broker: 'mqtts://velamq.prod:8883',
      clientId: 'edge-dev-runtime',
      flushIntervalMs: 1000,
      qos: 1,
      sinkId: 'velamq-main',
      topicTemplate: 'edge/{edge_id}/device/{device_id}/telemetry',
    });

    render(<EdgeNodesPage edges={edges} onSaveMqttUplink={onSaveMqttUplink} />);

    fireEvent.click(screen.getByRole('button', { name: 'MQTT 配置 edge-dev' }));
    const dialog = screen.getByRole('dialog', { name: '边端 MQTT 配置' });
    fireEvent.change(screen.getByLabelText('Broker 地址'), {
      target: { value: 'mqtts://velamq.prod:8883' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(onSaveMqttUplink).toHaveBeenCalledWith(
        'edge-dev',
        expect.objectContaining({ broker: 'mqtts://velamq.prod:8883' }),
      );
    });
    expect(dialog).toBeInTheDocument();
  });

  it('does not expose manual registration, credential, or maintenance actions', () => {
    render(<EdgeNodesPage edges={edges} />);

    expect(screen.queryByRole('button', { name: '注册边端' })).not.toBeInTheDocument();
    expect(
      screen.getByText(
        '边端由 runtime 通过 EdgeLink 主动连接后自动登记。云端负责查看运行状态、进入边端配置，并维护该边端的 MQTT 上报连接。',
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '轮换凭证' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '维护模式' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '轮换凭证 edge-dev' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '维护模式 edge-dev' })).not.toBeInTheDocument();
  });

  it('allows removing unreported edges but protects active runtime edges', async () => {
    const onDeleteEdge = vi.fn().mockResolvedValue(undefined);

    render(
      <EdgeNodesPage
        edges={[
          edges[0],
          {
            ...edges[0],
            edgeId: 'edge-draft-2',
            displayName: '新边端待确认',
            heartbeat: '-',
            resources: '-',
            runtimeId: '-',
            status: '未上报',
          },
        ]}
        onDeleteEdge={onDeleteEdge}
      />,
    );

    expect(
      screen.queryByRole('button', { name: '移除边端 edge-dev' }),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '移除边端 edge-draft-2' }));

    await waitFor(() => {
      expect(onDeleteEdge).toHaveBeenCalledWith('edge-draft-2');
    });
    expect(await screen.findByText('已移除边端 edge-draft-2')).toBeInTheDocument();
  });

  it('paginates edge rows locally', () => {
    const manyEdges = Array.from({ length: 12 }, (_, index) => ({
      ...edges[0],
      edgeId: `edge-${index + 1}`,
    }));

    render(<EdgeNodesPage edges={manyEdges} pageSize={5} />);

    expect(screen.getByText('第 1 / 3 页')).toBeInTheDocument();
    expect(screen.getByText('edge-1')).toBeInTheDocument();
    expect(screen.queryByText('edge-6')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '下一页' }));

    expect(screen.getByText('第 2 / 3 页')).toBeInTheDocument();
    expect(screen.getByText('edge-6')).toBeInTheDocument();
    expect(screen.queryByText('edge-1')).not.toBeInTheDocument();
  });
});
