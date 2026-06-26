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
    expect(screen.getByText('发布指令已发送')).toBeInTheDocument();
  });
});
