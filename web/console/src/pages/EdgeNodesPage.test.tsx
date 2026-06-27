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
        edges={edges}
        onConfigureEdge={onConfigureEdge}
        onMonitorEdge={onMonitorEdge}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '配置边端 edge-dev' }));
    fireEvent.click(screen.getByRole('button', { name: '运行监控 edge-dev' }));

    expect(onConfigureEdge).toHaveBeenCalledWith('edge-dev');
    expect(onMonitorEdge).toHaveBeenCalledWith('edge-dev');
  });

  it('runs lifecycle toolbar actions through handlers', async () => {
    const onRotateCredentials = vi.fn().mockResolvedValue({
      credentialVersion: 'credential-v2',
    });
    const onEnableMaintenance = vi.fn().mockResolvedValue({
      status: '维护中',
    });

    render(
      <EdgeNodesPage
        edges={edges}
        onEnableMaintenance={onEnableMaintenance}
        onRotateCredentials={onRotateCredentials}
      />,
    );

    expect(screen.queryByRole('button', { name: '注册边端' })).not.toBeInTheDocument();
    expect(screen.getByText('runtime 连接后自动登记')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '轮换凭证' }));
    await waitFor(() => {
      expect(onRotateCredentials).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findByText('凭证已轮换 credential-v2')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '维护模式' }));
    await waitFor(() => {
      expect(onEnableMaintenance).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findByText('维护模式已启用 维护中')).toBeInTheDocument();
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
