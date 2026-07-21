import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
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
  it('exposes edge access and monitoring actions', () => {
    render(
      <EdgeNodesPage
        accessTokens={{ 'edge-dev': 'edge-token-edge-dev-test' }}
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
      />,
    );

    expect(screen.queryByRole('columnheader', { name: '配置摘要' })).not.toBeInTheDocument();
    expect(screen.queryByRole('columnheader', { name: 'CPU / 内存 / 磁盘' })).not.toBeInTheDocument();
    expect(screen.queryByLabelText('边端舰队态势')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '接入信息 edge-dev' }));
    const accessDialog = screen.getByRole('dialog', { name: '边端接入信息' });
    expect(accessDialog).toBeInTheDocument();
    expect(within(accessDialog).getByText('edge-token-edge-dev-test')).toBeInTheDocument();
    fireEvent.click(within(accessDialog).getByRole('button', { name: '关闭' }));
    expect(screen.queryByRole('button', { name: '配置 edge-dev' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '运行监控 edge-dev' }));
    const monitorDialog = screen.getByRole('dialog', { name: '边端运行监控' });
    expect(within(monitorDialog).getByText('18.5%')).toBeInTheDocument();
    expect(within(monitorDialog).getByText('2')).toBeInTheDocument();
    expect(within(monitorDialog).getByText('已发布')).toBeInTheDocument();
  });

  it('creates an edge by selecting a product and generating an access token', async () => {
    const onCreateEdge = vi.fn().mockResolvedValue({
      ...edges[0],
      edgeId: 'edge-draft-2',
      displayName: '一号线边端',
      runtimeId: '-',
      status: '未上报',
      accessToken: 'edge_created_secret',
    });

    render(
      <EdgeNodesPage
        edges={edges}
        onCreateEdge={onCreateEdge}
        products={[
          {
            productId: 'pump-product',
            productName: '泵站采集产品',
            projectId: 'demo-plant',
            projectName: 'demo-plant',
            version: 'v1.0.0',
          },
        ]}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '新增边端' }));
    const dialog = screen.getByRole('dialog', { name: '新增边端' });
    fireEvent.change(within(dialog).getByLabelText('边端名称'), {
      target: { value: '一号线边端' },
    });
    fireEvent.change(within(dialog).getByLabelText('站点/分组'), {
      target: { value: '制造/一号线' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '生成接入 token' }));

    await waitFor(() => {
      expect(onCreateEdge).toHaveBeenCalledWith({
        displayName: '一号线边端',
        productId: 'pump-product',
        projectId: 'demo-plant',
        site: '制造/一号线',
      });
    });
    expect(await screen.findByText('已创建边端 edge-draft-2，token 已生成')).toBeInTheDocument();
    const accessDialog = screen.getByRole('dialog', { name: '边端接入信息' });
    expect(within(accessDialog).getByText('edge_created_secret')).toBeInTheDocument();
  });

  it('shows an access token only after explicit regeneration', async () => {
    const onGenerateAccessToken = vi.fn().mockResolvedValue('edge_new_secret');

    render(
      <EdgeNodesPage
        edges={edges}
        onGenerateAccessToken={onGenerateAccessToken}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '接入信息 edge-dev' }));
    const dialog = screen.getByRole('dialog', { name: '边端接入信息' });
    expect(
      within(dialog).getByText('Token 仅在创建或重新生成时显示，Cloud 不保存明文。'),
    ).toBeInTheDocument();
    expect(
      within(dialog).queryByText(/edge-runtime --cloud-gateway-addr/),
    ).not.toBeInTheDocument();

    fireEvent.click(within(dialog).getByRole('button', { name: '重新生成 token' }));

    expect(await within(dialog).findByText('edge_new_secret')).toBeInTheDocument();
    expect(
      within(dialog).getByText(/edge-runtime --cloud-gateway-addr/),
    ).toBeInTheDocument();
    expect(within(dialog).getByRole('status')).toHaveTextContent(
      '新 token 已生成，旧 token 已失效',
    );
    expect(onGenerateAccessToken).toHaveBeenCalledWith('edge-dev');
  });

  it('keeps product configuration out of edge instance actions', () => {
    render(
      <EdgeNodesPage
        configSummaries={[
          {
            collectionTaskCount: 0,
            dataConfigCount: 0,
            edgeId: 'edge-dev',
            mqttSinkId: 'velamq-main',
            pointCount: 0,
            protocolCount: 1,
            releaseStatus: '待发布',
          },
        ]}
        edges={edges}
      />,
    );

    expect(screen.queryByRole('button', { name: '配置 edge-dev' })).not.toBeInTheDocument();
    expect(screen.queryByRole('dialog', { name: '配置边端' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: '接入信息 edge-dev' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '运行监控 edge-dev' })).toBeInTheDocument();
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
      screen.getByText('手动登记边端，绑定产品，生成 runtime 接入 token。'),
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
