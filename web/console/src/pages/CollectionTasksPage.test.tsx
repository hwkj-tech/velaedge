import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { CollectionTasksPage } from './CollectionTasksPage';

describe('CollectionTasksPage', () => {
  it('shows collection task table and editor fields', () => {
    render(<CollectionTasksPage selectedEdgeId="edge-dev" />);

    expect(screen.getByText('任务清单')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '选择任务 pump-main' })).toBeInTheDocument();
    expect(screen.getAllByText('pressure, running').length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole('button', { name: '选择任务 pump-main' }));
    expect(screen.getByText('编辑任务 pump-main')).toBeInTheDocument();
    expect(screen.getByLabelText('采集周期(ms)')).toBeInTheDocument();
  });

  it('saves edited collection task drafts from the editor drawer', async () => {
    const onSaveTask = vi.fn().mockResolvedValue(undefined);

    render(<CollectionTasksPage selectedEdgeId="edge-dev" onSaveTask={onSaveTask} />);

    fireEvent.click(screen.getByRole('button', { name: '选择任务 pump-main' }));
    fireEvent.change(screen.getByLabelText('采集点位'), {
      target: { value: 'pressure' },
    });
    fireEvent.change(screen.getByLabelText('采集周期(ms)'), {
      target: { value: '2500' },
    });
    fireEvent.click(screen.getByLabelText('启用任务'));
    fireEvent.click(screen.getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(onSaveTask).toHaveBeenCalledWith('edge-dev', 'pump-main', {
        deviceId: 'pump-1',
        pointIds: ['pressure'],
        intervalMs: 2500,
        enabled: false,
      });
    });
    expect(screen.getByText('已保存')).toBeInTheDocument();
  });

  it('shows the bound edge context without switching edges in the page', () => {
    render(
      <CollectionTasksPage
        edges={[
          {
            edgeId: 'edge-dev',
            displayName: '研发实验室边端',
            site: '研发/实验室',
            runtimeId: 'runtime-dev',
            status: '健康',
            resources: '18% / 42% / 61%',
            heartbeat: '8 秒前',
            capabilities: ['protocol:modbus-tcp'],
          },
          {
            edgeId: 'edge-prod',
            displayName: '产线边端',
            site: '制造/一线',
            runtimeId: 'runtime-prod',
            status: '健康',
            resources: '22% / 48% / 66%',
            heartbeat: '6 秒前',
            capabilities: ['protocol:opcua'],
          },
        ]}
        selectedEdgeId="edge-dev"
      />,
    );

    expect(screen.getByLabelText('当前边端')).toHaveTextContent('研发实验室边端 / edge-dev');
    expect(screen.queryByLabelText('配置边端')).not.toBeInTheDocument();
  });

  it('hides the edge selector in sidebar list mode', () => {
    render(<CollectionTasksPage mode="list" selectedEdgeId="edge-dev" />);

    expect(screen.getByText('任务清单')).toBeInTheDocument();
    expect(screen.queryByLabelText('查看边端')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('配置边端')).not.toBeInTheDocument();
  });

  it('runs collection task toolbar actions through handlers', async () => {
    const onGenerateSchedule = vi.fn().mockResolvedValue({
      message: '调度策略已生成',
    });
    const onCreateTask = vi.fn().mockResolvedValue({
      taskId: 'task-draft-2',
    });

    render(
      <CollectionTasksPage
        selectedEdgeId="edge-dev"
        onCreateTask={onCreateTask}
        onGenerateSchedule={onGenerateSchedule}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '统一调度策略' }));
    await waitFor(() => {
      expect(onGenerateSchedule).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findByText('调度策略已生成')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '新建任务' }));
    const dialog = screen.getByRole('dialog', { name: '新建采集任务' });
    fireEvent.change(within(dialog).getByLabelText('新建 Task ID'), {
      target: { value: 'thermal-task' },
    });
    fireEvent.change(within(dialog).getByLabelText('新建任务设备 ID'), {
      target: { value: 'pump-1' },
    });
    fireEvent.change(within(dialog).getByLabelText('新建任务采集点位'), {
      target: { value: 'pressure' },
    });
    fireEvent.change(within(dialog).getByLabelText('新建任务采集周期(ms)'), {
      target: { value: '2000' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));
    await waitFor(() => {
      expect(onCreateTask).toHaveBeenCalledWith('edge-dev', {
        deviceId: 'pump-1',
        enabled: true,
        intervalMs: 2000,
        pointIds: ['pressure'],
        taskId: 'thermal-task',
      });
    });
    expect(await screen.findByText('已创建任务 task-draft-2')).toBeInTheDocument();
  });
});
