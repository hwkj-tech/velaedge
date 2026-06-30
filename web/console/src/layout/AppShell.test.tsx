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

    expect(screen.getByText('EdgeOps Cloud')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Dashboard/ })).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(screen.getByRole('button', { name: /边端管理/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /数据配置/ })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /算法配置/ })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: /MQTT Sink/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /配置发布/ })).toBeInTheDocument();
    expect(screen.getAllByText('Dashboard').length).toBeGreaterThan(0);
  });
});
