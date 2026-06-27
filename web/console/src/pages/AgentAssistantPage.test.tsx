import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { AgentAssistantPage } from './AgentAssistantPage';

describe('AgentAssistantPage', () => {
  it('runs agent actions through handlers and renders returned suggestions', async () => {
    const onGenerateSuggestions = vi.fn().mockResolvedValue({
      suggestions: [
        {
          detail: '根据 pump@v1 模型发现缺少 flow_rate 映射',
          state: '生成草稿',
          title: '点位补全',
        },
      ],
    });
    const onRunSafetyCheck = vi.fn().mockResolvedValue({
      status: '已通过',
    });

    render(
      <AgentAssistantPage
        onGenerateSuggestions={onGenerateSuggestions}
        onRunSafetyCheck={onRunSafetyCheck}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '安全策略' }));
    await waitFor(() => {
      expect(onRunSafetyCheck).toHaveBeenCalledOnce();
    });
    expect(await screen.findByText('安全策略检查 已通过')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '生成建议' }));
    await waitFor(() => {
      expect(onGenerateSuggestions).toHaveBeenCalledOnce();
    });
    expect(await screen.findByText('Agent 建议已生成 1 条')).toBeInTheDocument();
    expect(screen.getByText('点位补全')).toBeInTheDocument();
  });
});
