import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { AuditRecordResponse } from '../api/types';
import { AuditLogPage } from './AuditLogPage';

const records: AuditRecordResponse[] = [
  {
    action: 'create_release',
    actor: 'operator-a',
    createdAt: '2026-08-07T08:00:00Z',
    result: '成功',
    target: 'release-v2',
    time: '16:00:00',
  },
  {
    action: 'validate_config',
    actor: 'agent-service',
    createdAt: '2026-08-07T07:59:00Z',
    result: '失败',
    target: 'edge-lab-1',
    time: '15:59:00',
  },
  {
    action: 'bind_product',
    actor: 'operator-b',
    createdAt: '2026-08-07T07:58:00Z',
    result: '成功',
    target: 'pump-product',
    time: '15:58:00',
  },
];

describe('AuditLogPage', () => {
  it('filters, paginates, and opens a read-only event detail', () => {
    render(<AuditLogPage auditRecords={records} pageSize={2} />);

    expect(screen.getByText('第 1 / 2 页')).toBeInTheDocument();
    expect(screen.getByRole('cell', { name: 'create_release' })).toBeInTheDocument();
    expect(screen.queryByRole('cell', { name: 'bind_product' })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '下一页' }));
    expect(screen.getByRole('cell', { name: 'bind_product' })).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('搜索审计日志'), {
      target: { value: 'edge-lab-1' },
    });
    expect(screen.getByRole('cell', { name: 'validate_config' })).toBeInTheDocument();
    expect(screen.queryByRole('cell', { name: 'bind_product' })).not.toBeInTheDocument();

    fireEvent.click(
      screen.getByRole('button', {
        name: '查看审计事件 validate_config edge-lab-1',
      }),
    );
    const dialog = screen.getByRole('dialog', { name: '审计事件详情' });
    expect(within(dialog).getByText('agent-service')).toBeInTheDocument();
    expect(within(dialog).getAllByText('edge-lab-1')).toHaveLength(2);
    fireEvent.click(within(dialog).getByRole('button', { name: '关闭' }));
    expect(screen.queryByRole('dialog', { name: '审计事件详情' })).not.toBeInTheDocument();
  });

  it('filters by action and result', () => {
    render(<AuditLogPage auditRecords={records} />);

    fireEvent.change(screen.getByLabelText('动作筛选'), {
      target: { value: 'validate_config' },
    });
    expect(screen.getByRole('cell', { name: 'validate_config' })).toBeInTheDocument();
    expect(screen.queryByRole('cell', { name: 'create_release' })).not.toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('结果筛选'), {
      target: { value: '成功' },
    });
    expect(screen.getByText('没有符合筛选条件的审计记录')).toBeInTheDocument();
  });

  it('refreshes records from the cloud callback', async () => {
    const onRefresh = vi.fn().mockResolvedValue([records[0]]);
    render(<AuditLogPage auditRecords={[]} onRefresh={onRefresh} />);

    fireEvent.click(screen.getByRole('button', { name: '刷新' }));

    await waitFor(() => expect(onRefresh).toHaveBeenCalledOnce());
    expect(await screen.findByRole('cell', { name: 'create_release' })).toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('已同步 1 条审计记录');
  });
});
