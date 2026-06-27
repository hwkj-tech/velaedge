import { fireEvent, render, screen } from '@testing-library/react';
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
});
