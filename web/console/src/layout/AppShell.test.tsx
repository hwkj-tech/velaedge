import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { AppShell } from './AppShell';

describe('AppShell', () => {
  it('renders cloud console navigation and active content', () => {
    render(
      <AppShell activePage="dashboard" onNavigate={vi.fn()}>
        <h2>Dashboard</h2>
      </AppShell>,
    );

    expect(screen.getByText('VelaEdge')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Dashboard/ })).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(screen.getByRole('button', { name: /边端管理/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /项目管理/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /产品管理/ })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /设备模型/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /点位探测/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /协议连接/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /数据上报/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /配置发布/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /算法配置/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /MQTT Sink/ })).not.toBeInTheDocument();
    expect(screen.getAllByText('Dashboard').length).toBeGreaterThan(0);
  });

  it('renders platform status from live summary data', () => {
    render(
      <AppShell
        activePage="dashboard"
        onNavigate={vi.fn()}
        platformStatus={{
          environment: 'prod',
          onlineEdgeCount: 12,
          pendingReleaseCount: 4,
          project: 'factory-a',
        }}
      >
        <h2>Dashboard</h2>
      </AppShell>,
    );

    expect(screen.getByText('12 个边端在线')).toBeInTheDocument();
    expect(screen.getByText('项目: factory-a')).toBeInTheDocument();
    expect(screen.getByText('环境: prod')).toBeInTheDocument();
    expect(screen.getByText('4 个配置待发布')).toBeInTheDocument();
  });

  it('shows the authenticated principal and supports logout', () => {
    const onLogout = vi.fn();
    render(
      <AppShell
        activePage="dashboard"
        onLogout={onLogout}
        onNavigate={vi.fn()}
        principal={{
          authenticationEnabled: true,
          role: 'operator',
          subject: 'plant-operator',
        }}
      >
        <h2>Dashboard</h2>
      </AppShell>,
    );

    expect(screen.getByText('plant-operator')).toBeInTheDocument();
    expect(screen.getByText('操作员')).toBeInTheDocument();
    screen.getByRole('button', { name: '退出控制台' }).click();
    expect(onLogout).toHaveBeenCalledOnce();
  });
});
