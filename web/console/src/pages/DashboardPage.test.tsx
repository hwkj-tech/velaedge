import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { DashboardPage } from './DashboardPage';

describe('DashboardPage', () => {
  it('runs dashboard quick actions through handlers', async () => {
    const onCreatePoint = vi.fn().mockResolvedValue({
      pointId: 'point-draft-3',
    });
    const onPublish = vi.fn().mockResolvedValue(undefined);

    render(
      <DashboardPage
        loadState="ready"
        summary={{ edge_count: 1, pending_release_count: 0 }}
        onCreatePoint={onCreatePoint}
        onPublish={onPublish}
      />,
    );

    expect(screen.queryByRole('button', { name: '注册边端' })).not.toBeInTheDocument();
    expect(screen.getByText('边端连接后自动发现')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '创建点位' }));
    await waitFor(() => {
      expect(onCreatePoint).toHaveBeenCalledOnce();
    });
    expect(await screen.findByText('已创建点位草稿 point-draft-3')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '发布配置' }));
    await waitFor(() => {
      expect(onPublish).toHaveBeenCalledOnce();
    });
    expect(await screen.findByText('已创建发布，等待 runtime 回报')).toBeInTheDocument();
  });
});
