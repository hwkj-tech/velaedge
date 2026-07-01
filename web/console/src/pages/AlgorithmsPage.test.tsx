import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { AlgorithmsPage } from './AlgorithmsPage';

describe('AlgorithmsPage', () => {
  it('shows algorithm table and editor fields', () => {
    render(<AlgorithmsPage selectedEdgeId="edge-dev" />);

    expect(screen.getByRole('heading', { name: '算法模板', level: 3 })).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: '选择算法 pressure-change-report' }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '选择算法 pressure-change-report' }));
    expect(screen.getByText('编辑算法 pressure-change-report')).toBeInTheDocument();
    expect(screen.getByLabelText('算法类型')).toBeInTheDocument();
    expect(screen.getByLabelText('DSL 预览')).toHaveTextContent('changeFilter');
  });

  it('shows an explicit row action for editing algorithms', () => {
    render(<AlgorithmsPage selectedEdgeId="edge-dev" />);

    expect(screen.getByRole('columnheader', { name: '操作' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '修改算法 pressure-change-report' }));

    expect(screen.getByRole('dialog', { name: '编辑算法 pressure-change-report' })).toBeInTheDocument();
    expect(screen.getByLabelText('算法类型')).toHaveValue('ChangeReport');
  });

  it('saves edited algorithm drafts from the editor drawer', async () => {
    const onSaveAlgorithm = vi.fn().mockResolvedValue(undefined);

    render(
      <AlgorithmsPage selectedEdgeId="edge-dev" onSaveAlgorithm={onSaveAlgorithm} />,
    );

    fireEvent.click(screen.getByRole('button', { name: '选择算法 pressure-change-report' }));
    fireEvent.change(screen.getByLabelText('算法类型'), {
      target: { value: 'WindowAggregate' },
    });
    fireEvent.change(screen.getByLabelText('输入点位'), {
      target: { value: 'pressure' },
    });
    fireEvent.change(screen.getByLabelText('输出虚拟点位'), {
      target: { value: 'pressure.avg_1m' },
    });
    fireEvent.change(screen.getByLabelText('窗口大小(ms)'), {
      target: { value: '60000' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(onSaveAlgorithm).toHaveBeenCalledWith(
        'edge-dev',
        'pressure-change-report',
        {
          version: '1.0.0',
          algorithmKind: 'WindowAggregate',
          dsl: {
            inputs: [{ alias: 'p', pointId: 'pressure' }],
            trigger: { type: 'window', everyMs: 60000 },
            steps: [
              {
                type: 'windowAggregate',
                source: 'p',
                functions: [{ function: 'avg', output: 'avg_1m' }],
              },
            ],
            outputs: [{ name: 'avg_1m', pointId: 'pressure.avg_1m' }],
            report: { mode: 'WindowResult', sink: 'velamq-main' },
          },
        },
      );
    });
    expect(screen.getByText('已保存')).toBeInTheDocument();
  });

  it('shows the bound edge context without switching edges in the page', () => {
    render(
      <AlgorithmsPage
        edges={[
          {
            edgeId: 'edge-dev',
            displayName: '研发实验室边端',
            site: '研发/实验室',
            runtimeId: 'runtime-dev',
            status: '健康',
            resources: '18% / 42% / 61%',
            heartbeat: '8 秒前',
            capabilities: ['algorithm:dsl'],
          },
          {
            edgeId: 'edge-prod',
            displayName: '产线边端',
            site: '制造/一线',
            runtimeId: 'runtime-prod',
            status: '健康',
            resources: '22% / 48% / 66%',
            heartbeat: '6 秒前',
            capabilities: ['algorithm:dsl'],
          },
        ]}
        selectedEdgeId="edge-dev"
      />,
    );

    expect(screen.queryByLabelText('当前边端')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('配置边端')).not.toBeInTheDocument();
  });

  it('hides the edge selector in sidebar list mode', () => {
    render(<AlgorithmsPage mode="list" selectedEdgeId="edge-dev" />);

    expect(screen.getByText('算法模板')).toBeInTheDocument();
    expect(screen.queryByLabelText('查看边端')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('配置边端')).not.toBeInTheDocument();
  });

  it('runs algorithm toolbar actions through handlers', async () => {
    const onAssessRisk = vi.fn().mockResolvedValue({
      status: '已通过',
    });
    const onCreateAlgorithm = vi.fn().mockResolvedValue({
      algorithmId: 'algorithm-draft-2',
    });

    render(
      <AlgorithmsPage
        selectedEdgeId="edge-dev"
        onAssessRisk={onAssessRisk}
        onCreateAlgorithm={onCreateAlgorithm}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '风险评估' }));
    await waitFor(() => {
      expect(onAssessRisk).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findByText('算法风险评估 已通过')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '新建算法' }));
    const dialog = screen.getByRole('dialog', { name: '新建算法' });
    fireEvent.change(within(dialog).getByLabelText('新建 Algorithm ID'), {
      target: { value: 'thermal-rule' },
    });
    fireEvent.change(within(dialog).getByLabelText('算法版本'), {
      target: { value: '1.0.0' },
    });
    fireEvent.change(within(dialog).getByLabelText('算法类型'), {
      target: { value: 'ThresholdRule' },
    });
    fireEvent.change(within(dialog).getByLabelText('输入点位'), {
      target: { value: 'pressure' },
    });
    fireEvent.change(within(dialog).getByLabelText('输出虚拟点位'), {
      target: { value: 'thermal.alert' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: '保存' }));
    await waitFor(() => {
      expect(onCreateAlgorithm).toHaveBeenCalledWith('edge-dev', {
        algorithmId: 'thermal-rule',
        version: '1.0.0',
        algorithmKind: 'ThresholdRule',
        dsl: {
          inputs: [{ alias: 'p', pointId: 'pressure' }],
          trigger: { type: 'onSample' },
          steps: [
            {
              type: 'thresholdRule',
              source: 'p',
              operator: 'Gt',
              threshold: 0.2,
              event: {
                code: 'THERMAL.ALERT_ALARM',
                severity: 'Warning',
                message: '算法阈值告警',
              },
            },
          ],
          outputs: [{ name: 'alert', pointId: 'thermal.alert' }],
          report: { mode: 'EventOnly', sink: 'velamq-main' },
        },
      });
    });
    expect(await screen.findByText('已创建算法 algorithm-draft-2')).toBeInTheDocument();
  });
});
