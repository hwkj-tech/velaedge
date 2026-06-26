import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { PointMappingsPage } from './PointMappingsPage';

describe('PointMappingsPage', () => {
  it('shows point table and editor drawer fields', () => {
    render(<PointMappingsPage />);

    expect(screen.getByText('点位配置表')).toBeInTheDocument();
    expect(screen.getByText('pressure')).toBeInTheDocument();
    expect(screen.getByText('holding_register:40001')).toBeInTheDocument();
    expect(screen.getByText('编辑点位 pressure')).toBeInTheDocument();
    expect(screen.getByText('采集周期')).toBeInTheDocument();
  });

  it('saves edited point mapping drafts from the editor drawer', async () => {
    const onSavePoint = vi.fn().mockResolvedValue(undefined);

    render(<PointMappingsPage onSavePoint={onSavePoint} />);

    fireEvent.change(screen.getByLabelText('地址值'), {
      target: { value: '40002' },
    });
    fireEvent.change(screen.getByLabelText('采集周期(ms)'), {
      target: { value: '2000' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存草稿' }));

    await waitFor(() => {
      expect(onSavePoint).toHaveBeenCalledWith('pressure', {
        addressKind: 'holding_register',
        addressValue: '40002',
        intervalMs: 2000,
        unit: 'MPa',
      });
    });
    expect(screen.getByText('草稿已保存')).toBeInTheDocument();
  });
});
