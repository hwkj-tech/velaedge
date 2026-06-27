import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { DeviceModelsPage } from './DeviceModelsPage';

describe('DeviceModelsPage', () => {
  it('creates device model drafts through the page action', async () => {
    const onCreateDeviceModel = vi.fn().mockResolvedValue({
      deviceType: 'device-model-draft-2',
    });

    render(<DeviceModelsPage onCreateDeviceModel={onCreateDeviceModel} />);

    fireEvent.click(screen.getByRole('button', { name: '新建设备模型' }));

    await waitFor(() => {
      expect(onCreateDeviceModel).toHaveBeenCalledOnce();
    });
    expect(
      await screen.findByText('已创建设备模型草稿 device-model-draft-2'),
    ).toBeInTheDocument();
  });
});
