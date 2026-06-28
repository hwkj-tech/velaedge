import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { AlgorithmsPage } from './AlgorithmsPage';

describe('AlgorithmsPage', () => {
  it('shows algorithm table and editor fields', () => {
    render(<AlgorithmsPage selectedEdgeId="edge-dev" />);

    expect(screen.getByText('算法模板')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: '选择算法 pump-anomaly-v1' }),
    ).toBeInTheDocument();
    expect(screen.getByText('编辑算法 pump-anomaly-v1')).toBeInTheDocument();
    expect(screen.getByLabelText('算法运行时')).toBeInTheDocument();
  });

  it('saves edited algorithm drafts from the editor drawer', async () => {
    const onSaveAlgorithm = vi.fn().mockResolvedValue(undefined);

    render(
      <AlgorithmsPage selectedEdgeId="edge-dev" onSaveAlgorithm={onSaveAlgorithm} />,
    );

    fireEvent.change(screen.getByLabelText('算法版本'), {
      target: { value: '1.1.0' },
    });
    fireEvent.change(screen.getByLabelText('算法运行时'), {
      target: { value: 'Wasm' },
    });
    fireEvent.change(screen.getByLabelText('输入点位'), {
      target: { value: 'pressure' },
    });
    fireEvent.change(screen.getByLabelText('输出变量'), {
      target: { value: 'pump.pressure_score' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(onSaveAlgorithm).toHaveBeenCalledWith(
        'edge-dev',
        'pump-anomaly-v1',
        {
          version: '1.1.0',
          runtime: 'Wasm',
          inputIds: ['pressure'],
          outputIds: ['pump.pressure_score'],
        },
      );
    });
    expect(screen.getByText('已保存')).toBeInTheDocument();
  });

  it('switches the active edge before editing algorithms', async () => {
    const onSelectEdge = vi.fn().mockResolvedValue(undefined);

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
            capabilities: ['algorithm:onnx'],
          },
          {
            edgeId: 'edge-prod',
            displayName: '产线边端',
            site: '制造/一线',
            runtimeId: 'runtime-prod',
            status: '健康',
            resources: '22% / 48% / 66%',
            heartbeat: '6 秒前',
            capabilities: ['algorithm:wasm'],
          },
        ]}
        selectedEdgeId="edge-dev"
        onSelectEdge={onSelectEdge}
      />,
    );

    fireEvent.change(screen.getByLabelText('配置边端'), {
      target: { value: 'edge-prod' },
    });

    await waitFor(() => {
      expect(onSelectEdge).toHaveBeenCalledWith('edge-prod');
    });
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
    await waitFor(() => {
      expect(onCreateAlgorithm).toHaveBeenCalledWith('edge-dev');
    });
    expect(await screen.findByText('已创建算法草稿 algorithm-draft-2')).toBeInTheDocument();
  });
});
