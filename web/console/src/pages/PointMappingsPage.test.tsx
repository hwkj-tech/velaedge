import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

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
});
