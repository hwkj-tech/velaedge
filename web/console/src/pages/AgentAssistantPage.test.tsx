import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { AgentAssistantPage } from './AgentAssistantPage';

describe('AgentAssistantPage', () => {
  it('runs agent actions through handlers and renders returned suggestions', async () => {
    const onGenerateSuggestions = vi.fn().mockResolvedValue({
      suggestions: [
        {
          detail: '根据 pump@v1 模型发现缺少 flow_rate 映射',
          state: '生成候选配置',
          title: '点位补全',
        },
      ],
    });
    const onRunSafetyCheck = vi.fn().mockResolvedValue({
      status: '已通过',
    });
    const onChat = vi.fn().mockResolvedValue({
      citations: [
        {
          documentId: 'knowledge-1',
          excerpt: '超时后检查串口参数。',
          sourceUri: 'kb://manual/modbus',
          title: 'Modbus 运维手册',
        },
      ],
      message: '该边端需要先校验配置差异，再保存草案并人工审核。',
      mode: 'openai_compatible',
      model: 'edgeops-test-model',
    });
    const onGetProviderStatus = vi.fn().mockResolvedValue({
      configured: true,
      mode: 'openai_compatible',
      model: 'edgeops-test-model',
    });

    render(
      <AgentAssistantPage
        onChat={onChat}
        onGenerateSuggestions={onGenerateSuggestions}
        onGetProviderStatus={onGetProviderStatus}
        onRunSafetyCheck={onRunSafetyCheck}
        projectOptions={[{ projectId: 'demo-plant', projectName: 'Demo Plant' }]}
      />,
    );

    expect(screen.getByText('云边配置助手')).toBeInTheDocument();
    expect(screen.getByText('Agent 助手已就绪')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '安全策略检查' }));
    await waitFor(() => {
      expect(onRunSafetyCheck).toHaveBeenCalledOnce();
    });
    expect(await screen.findByText('安全策略结果')).toBeInTheDocument();
    expect(screen.getByText(/安全策略检查 已通过/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '生成候选建议' }));
    await waitFor(() => {
      expect(onGenerateSuggestions).toHaveBeenCalledOnce();
    });
    expect(screen.getAllByText('候选建议').length).toBeGreaterThan(0);
    expect(screen.getByText('已生成 1 条候选建议。建议只进入候选队列，不会自动修改配置。')).toBeInTheDocument();
    expect(screen.getAllByText('点位补全').length).toBeGreaterThan(0);

    fireEvent.change(screen.getByLabelText('输入 Agent 问题'), {
      target: { value: '为什么 edge-dev 需要先校验再发布？' },
    });
    fireEvent.click(screen.getByRole('button', { name: '发送' }));

    expect(screen.getByText('为什么 edge-dev 需要先校验再发布？')).toBeInTheDocument();
    await waitFor(() => expect(onChat).toHaveBeenCalledOnce());
    expect(onChat).toHaveBeenCalledWith({
      conversationId: undefined,
      message: '为什么 edge-dev 需要先校验再发布？',
      operatorId: 'console-operator',
      projectId: 'demo-plant',
    });
    expect(await screen.findByText('模型分析')).toBeInTheDocument();
    expect(
      screen.getByText('该边端需要先校验配置差异，再保存草案并人工审核。'),
    ).toBeInTheDocument();
    expect(screen.getByText('edgeops-test-model')).toBeInTheDocument();
    expect(screen.getByText('Modbus 运维手册')).toBeInTheDocument();
    expect(screen.getByText('kb://manual/modbus')).toBeInTheDocument();
  });

  it('saves suggestions as governed proposals and reviews without publishing', async () => {
    const pendingProposal = {
      agentId: 'edgeops-agent',
      createdAt: '2026-07-16T04:00:00Z',
      createdBy: 'console-operator',
      edgeId: null,
      kind: 'point_mapping' as const,
      payload: {},
      projectId: null,
      proposalId: 'proposal-1',
      reviewNote: null,
      reviewedAt: null,
      reviewedBy: null,
      risk: 'low' as const,
      status: 'pending_review' as const,
      summary: '根据 pump@v1 模型发现缺少 flow_rate 映射',
      title: '点位补全',
    };
    const onCreateProposal = vi.fn().mockResolvedValue(pendingProposal);
    const onListProposals = vi.fn().mockResolvedValue([]);
    const onReviewProposal = vi.fn().mockResolvedValue({
      ...pendingProposal,
      reviewNote: '允许进入人工配置流程，不自动发布',
      reviewedAt: '2026-07-16T04:01:00Z',
      reviewedBy: 'console-reviewer',
      status: 'approved',
    });

    render(
      <AgentAssistantPage
        onCreateProposal={onCreateProposal}
        onListProposals={onListProposals}
        onReviewProposal={onReviewProposal}
      />,
    );

    fireEvent.click(
      screen.getByRole('button', { name: '保存 点位补全 为审核草案' }),
    );
    await waitFor(() => expect(onCreateProposal).toHaveBeenCalledOnce());
    expect(await screen.findByText('草案已保存')).toBeInTheDocument();
    expect(screen.getByText('待审核')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '通过 点位补全' }));
    await waitFor(() => expect(onReviewProposal).toHaveBeenCalledOnce());
    expect(screen.getByText('已通过')).toBeInTheDocument();
    expect(screen.getByText('console-reviewer')).toBeInTheDocument();
  });

  it('creates and deletes project-scoped governed knowledge', async () => {
    const document = {
      content: '超时后检查串口参数。',
      createdAt: '2026-07-17T00:00:00Z',
      createdBy: 'console-operator',
      documentId: 'knowledge-1',
      enabled: true,
      projectId: 'demo-plant',
      sourceUri: 'kb://manual/modbus',
      tags: ['Modbus', '运维'],
      title: 'Modbus 运维手册',
      updatedAt: '2026-07-17T00:00:00Z',
    };
    const onListKnowledge = vi.fn().mockResolvedValue([]);
    const onSaveKnowledge = vi.fn().mockResolvedValue(document);
    const onDeleteKnowledge = vi.fn().mockResolvedValue(undefined);

    render(
      <AgentAssistantPage
        onDeleteKnowledge={onDeleteKnowledge}
        onListKnowledge={onListKnowledge}
        onSaveKnowledge={onSaveKnowledge}
        projectOptions={[{ projectId: 'demo-plant', projectName: 'Demo Plant' }]}
      />,
    );

    await waitFor(() =>
      expect(onListKnowledge).toHaveBeenCalledWith('demo-plant'),
    );
    fireEvent.click(screen.getByRole('button', { name: '新增知识条目' }));
    fireEvent.change(screen.getByLabelText('知识标题'), {
      target: { value: 'Modbus 运维手册' },
    });
    fireEvent.change(screen.getByLabelText('知识来源标识'), {
      target: { value: 'kb://manual/modbus' },
    });
    fireEvent.change(screen.getByLabelText('知识标签'), {
      target: { value: 'Modbus, 运维' },
    });
    fireEvent.change(screen.getByLabelText('知识正文'), {
      target: { value: '超时后检查串口参数。' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));

    await waitFor(() => expect(onSaveKnowledge).toHaveBeenCalledOnce());
    expect(onSaveKnowledge).toHaveBeenCalledWith(
      null,
      expect.objectContaining({
        projectId: 'demo-plant',
        tags: ['Modbus', '运维'],
        title: 'Modbus 运维手册',
      }),
    );
    expect(await screen.findByText('Modbus 运维手册')).toBeInTheDocument();

    fireEvent.click(
      screen.getByRole('button', { name: '删除知识 Modbus 运维手册' }),
    );
    fireEvent.click(
      screen.getByRole('button', { name: '确认删除知识 Modbus 运维手册' }),
    );
    await waitFor(() => expect(onDeleteKnowledge).toHaveBeenCalledWith('knowledge-1'));
    expect(screen.queryByText('Modbus 运维手册')).not.toBeInTheDocument();
  });

  it('restores, continues and deletes operator-scoped conversations', async () => {
    const conversation = {
      conversationId: 'conversation-1',
      createdAt: '2026-07-17T01:00:00Z',
      edgeId: null,
      messages: [
        {
          citations: [],
          content: '检查 edge-dev 的发布风险',
          createdAt: '2026-07-17T01:00:00Z',
          messageId: 'message-1',
          role: 'user' as const,
        },
        {
          citations: [],
          content: '建议先校验配置差异。',
          createdAt: '2026-07-17T01:00:01Z',
          messageId: 'message-2',
          role: 'assistant' as const,
        },
      ],
      operatorId: 'console-operator',
      projectId: 'demo-plant',
      title: '检查 edge-dev 的发布风险',
      updatedAt: '2026-07-17T01:00:01Z',
    };
    const onListConversations = vi.fn().mockResolvedValue([conversation]);
    const onDeleteConversation = vi.fn().mockResolvedValue(undefined);
    const onChat = vi.fn().mockResolvedValue({
      citations: [],
      conversationId: 'conversation-1',
      conversationTitle: conversation.title,
      message: '当前没有阻塞项。',
      mode: 'deterministic',
      model: 'edgeops-local-analysis',
    });

    const { rerender } = render(
      <AgentAssistantPage
        onChat={onChat}
        onDeleteConversation={onDeleteConversation}
        onListConversations={onListConversations}
        projectOptions={[{ projectId: 'demo-plant', projectName: 'Demo Plant' }]}
      />,
    );

    await waitFor(() =>
      expect(onListConversations).toHaveBeenCalledWith('demo-plant'),
    );
    fireEvent.change(screen.getByLabelText('Agent 历史会话'), {
      target: { value: 'conversation-1' },
    });
    expect(screen.getAllByText('检查 edge-dev 的发布风险')).toHaveLength(2);
    expect(screen.getByText('建议先校验配置差异。')).toBeInTheDocument();

    const refreshedListHandler = vi.fn().mockResolvedValue([conversation]);
    rerender(
      <AgentAssistantPage
        onChat={onChat}
        onDeleteConversation={onDeleteConversation}
        onListConversations={refreshedListHandler}
        projectOptions={[{ projectId: 'demo-plant', projectName: 'Demo Plant' }]}
      />,
    );
    expect(screen.getByLabelText('Agent 历史会话')).toHaveValue('conversation-1');
    expect(screen.getByText('建议先校验配置差异。')).toBeInTheDocument();
    expect(refreshedListHandler).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText('输入 Agent 问题'), {
      target: { value: '还有阻塞项吗？' },
    });
    fireEvent.click(screen.getByRole('button', { name: '发送' }));
    await waitFor(() =>
      expect(onChat).toHaveBeenCalledWith({
        conversationId: 'conversation-1',
        message: '还有阻塞项吗？',
        operatorId: 'console-operator',
        projectId: 'demo-plant',
      }),
    );

    fireEvent.click(screen.getByRole('button', { name: '删除当前 Agent 会话' }));
    fireEvent.click(
      screen.getByRole('button', { name: '确认删除当前 Agent 会话' }),
    );
    await waitFor(() =>
      expect(onDeleteConversation).toHaveBeenCalledWith('conversation-1'),
    );
    expect(screen.getByText('Agent 助手已就绪')).toBeInTheDocument();
  });
});
