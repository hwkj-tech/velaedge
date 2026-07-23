import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ReleasesPage } from './ReleasesPage';

describe('ReleasesPage', () => {
  it('publishes the latest draft through the page action', async () => {
    const onPublish = vi.fn().mockResolvedValue(undefined);

    render(<ReleasesPage selectedEdgeId="edge-dev" onPublish={onPublish} />);

    fireEvent.click(screen.getByRole('button', { name: '创建发布' }));

    await waitFor(() => {
      expect(onPublish).toHaveBeenCalledOnce();
    });
    expect(screen.getByText('已创建发布，等待 runtime 回报')).toBeInTheDocument();
  });

  it('does not expose a global edge selector for publishing', async () => {
    const onPublish = vi.fn().mockResolvedValue(undefined);

    render(
      <ReleasesPage
        edges={[
          {
            edgeId: 'edge-dev',
            displayName: '研发边端',
            site: '研发',
            runtimeId: 'runtime-dev',
            status: '健康',
            resources: '10% / 20% / 30%',
            heartbeat: '8 秒前',
            capabilities: [],
          },
          {
            edgeId: 'edge-lab',
            displayName: '实验边端',
            site: '实验室',
            runtimeId: 'runtime-lab',
            status: '健康',
            resources: '11% / 22% / 33%',
            heartbeat: '12 秒前',
            capabilities: [],
          },
        ]}
        onPublish={onPublish}
      />,
    );

    expect(screen.queryByLabelText('发布边端')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '创建发布' }));

    await waitFor(() => {
      expect(onPublish).toHaveBeenCalledWith('edge-dev');
    });
  });

  it('runs release toolbar actions through handlers', async () => {
    const onShowDiff = vi.fn().mockResolvedValue({
      message: '配置差异摘要已生成',
    });
    const onValidateRelease = vi.fn().mockResolvedValue({
      status: '已通过',
    });

    render(
      <ReleasesPage
        selectedEdgeId="edge-dev"
        onShowDiff={onShowDiff}
        onValidateRelease={onValidateRelease}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '查看差异' }));
    await waitFor(() => {
      expect(onShowDiff).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findByText('配置差异摘要已生成')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '校验配置' }));
    await waitFor(() => {
      expect(onValidateRelease).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findByText('发布配置校验 已通过')).toBeInTheDocument();
  });
});
