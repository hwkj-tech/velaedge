import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ReleasesPage } from './ReleasesPage';

describe('ReleasesPage', () => {
  it('publishes the latest draft through the page action', async () => {
    const onPublish = vi.fn().mockResolvedValue(undefined);

    render(<ReleasesPage onPublish={onPublish} />);

    fireEvent.click(screen.getByRole('button', { name: '创建发布' }));

    await waitFor(() => {
      expect(onPublish).toHaveBeenCalledOnce();
    });
    expect(screen.getByText('已创建发布，等待 runtime 回报')).toBeInTheDocument();
  });

  it('publishes to the selected edge node', async () => {
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

    fireEvent.change(screen.getByLabelText('发布边端'), {
      target: { value: 'edge-lab' },
    });
    fireEvent.click(screen.getByRole('button', { name: '创建发布' }));

    await waitFor(() => {
      expect(onPublish).toHaveBeenCalledWith('edge-lab');
    });
  });
});
