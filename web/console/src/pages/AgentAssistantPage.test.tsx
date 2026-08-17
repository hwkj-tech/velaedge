import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { AgentAssistantPage } from './AgentAssistantPage';
import type {
  AgentChangeSetResponse,
  AgentCommandCandidateResponse,
} from '../api/types';

const validGovernanceReport = {
  checkedAt: '2026-08-10T10:00:00Z',
  impact: {
    affectedEdges: ['edge-dev'],
    affectedResources: 1,
    commandPathChanged: false,
    notes: ['Runtime 将实时接收配置'],
    requiresRuntimeSync: true,
  },
  issues: [],
  valid: true,
};

function changeSet(status: AgentChangeSetResponse['status']): AgentChangeSetResponse {
  return {
    baseRevision: 'revision-1',
    changeSetId: 'change-1',
    confirmation: status === 'confirmed' || status === 'applied' ? {
      confirmedAt: '2026-08-10T10:01:00Z',
      confirmedBy: 'console-operator',
      note: '窗口已确认',
    } : null,
    createdAt: '2026-08-10T10:00:00Z',
    createdBy: 'edgeops-agent',
    operations: [{
      after: { intervalMs: 500 },
      before: { intervalMs: 1000 },
      dependsOn: [],
      kind: 'update',
      operationId: 'operation-1',
      resourceId: 'pump-task',
      resourceKind: 'collection_task',
    }],
    rationale: '降低压力采样延迟，并保持 Runtime 实时同步。',
    risk: 'medium',
    status,
    target: {
      edgeIds: ['edge-dev'],
      productId: 'pump-product',
      projectId: 'demo-plant',
    },
    title: '调整压力采样周期',
    updatedAt: '2026-08-10T10:01:00Z',
    validation: validGovernanceReport,
  };
}

function commandCandidate(
  status: AgentCommandCandidateResponse['status'],
): AgentCommandCandidateResponse {
  return {
    candidateId: 'command-1',
    confirmation: status === 'confirmed' || status === 'dispatched' ? {
      confirmedAt: '2026-08-10T10:02:00Z',
      confirmedBy: 'console-operator',
      note: '现场已清场',
    } : null,
    createdAt: '2026-08-10T10:00:00Z',
    createdBy: 'edgeops-agent',
    idempotencyKey: 'command-edge-dev-pump-start-1',
    rationale: '启动备用泵前已校验可写点位与设备策略。',
    risk: 'high',
    status,
    target: {
      deviceId: 'pump-1',
      edgeId: 'edge-dev',
      flowId: 'pump-command-flow',
      pointId: 'start_command',
      productId: 'pump-product',
      projectId: 'demo-plant',
      protocolConnectionId: 'modbus-line-a',
    },
    title: '启动备用泵',
    updatedAt: '2026-08-10T10:02:00Z',
    validation: validGovernanceReport,
    value: true,
  };
}

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
      streaming: true,
      toolCalling: true,
    });
    const onGetMetrics = vi.fn().mockResolvedValue({
      averageLatencyMs: 143,
      completionTokens: 1200,
      deterministicCount: 2,
      estimatedCostMicrousd: 4200,
      failedRequestCount: 1,
      fallbackCount: 2,
      lastLatencyMs: 90,
      promptTokens: 3000,
      providerAttemptCount: 14,
      providerSuccessCount: 10,
      requestCount: 12,
      securityBlockCount: 1,
      securityFilterCount: 3,
      toolCallCount: 8,
      toolFailureCount: 1,
      totalLatencyMs: 1716,
      totalTokens: 4200,
    });

    render(
      <AgentAssistantPage
        onChat={onChat}
        onGenerateSuggestions={onGenerateSuggestions}
        onGetMetrics={onGetMetrics}
        onGetProviderStatus={onGetProviderStatus}
        onRunSafetyCheck={onRunSafetyCheck}
        projectOptions={[{ projectId: 'demo-plant', projectName: 'Demo Plant' }]}
      />,
    );

    expect(screen.getByText('云边智能执行助手')).toBeInTheDocument();
    expect(screen.getByText('Agent 助手已就绪')).toBeInTheDocument();
    const metrics = await screen.findByLabelText('Agent 真实运行指标');
    expect(within(metrics).getByText('143 ms')).toBeInTheDocument();
    expect(within(metrics).getByText('1 拦截 · 3 清洗')).toBeInTheDocument();

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
    expect(screen.getByText(/edgeops-test-model/)).toBeInTheDocument();
    expect(screen.getByText('Modbus 运维手册')).toBeInTheDocument();
    expect(screen.getByText('kb://manual/modbus')).toBeInTheDocument();
    await waitFor(() => expect(onGetMetrics).toHaveBeenCalledTimes(3));
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
    const onGenerateSuggestions = vi.fn().mockResolvedValue({
      suggestions: [{
        detail: '根据 pump@v1 模型发现缺少 flow_rate 映射',
        state: '生成候选配置',
        title: '点位补全',
      }],
    });
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
        onGenerateSuggestions={onGenerateSuggestions}
        onListProposals={onListProposals}
        onReviewProposal={onReviewProposal}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '生成候选建议' }));
    await waitFor(() => expect(onGenerateSuggestions).toHaveBeenCalledOnce());
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

  it('keeps proposal review controls hidden from non-admin principals', async () => {
    const onReviewProposal = vi.fn();
    render(
      <AgentAssistantPage
        canReviewProposals={false}
        onListProposals={vi.fn().mockResolvedValue([
          {
            agentId: 'edgeops-agent',
            createdAt: '2026-07-16T04:00:00Z',
            createdBy: 'config-operator',
            edgeId: null,
            kind: 'config_suggestion',
            payload: {},
            projectId: null,
            proposalId: 'proposal-operator-review',
            reviewNote: null,
            reviewedAt: null,
            reviewedBy: null,
            risk: 'medium',
            status: 'pending_review',
            summary: '建议调整压力点采集周期',
            title: '调整采集周期',
          },
        ])}
        onReviewProposal={onReviewProposal}
      />,
    );

    expect(await screen.findByText('需要管理员审核')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '通过 调整采集周期' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '驳回 调整采集周期' })).not.toBeInTheDocument();
    expect(onReviewProposal).not.toHaveBeenCalled();
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

  it('simulates, confirms and applies a governed ChangeSet from the execution center', async () => {
    const onApplyChangeSet = vi.fn().mockResolvedValue(changeSet('applied'));
    const onConfirmChangeSet = vi.fn().mockResolvedValue(changeSet('confirmed'));
    const onListChangeSets = vi.fn().mockResolvedValue([changeSet('awaiting_confirmation')]);
    const onListCommandCandidates = vi.fn().mockResolvedValue([]);
    const onSimulateChangeSet = vi.fn().mockResolvedValue({
      applied: false,
      changeSet: changeSet('awaiting_confirmation'),
      packages: [{ edgeId: 'edge-dev', revision: 'revision-2' }],
    });

    render(
      <AgentAssistantPage
        onApplyChangeSet={onApplyChangeSet}
        onConfirmChangeSet={onConfirmChangeSet}
        onListChangeSets={onListChangeSets}
        onListCommandCandidates={onListCommandCandidates}
        onSimulateChangeSet={onSimulateChangeSet}
        projectOptions={[{ projectId: 'demo-plant', projectName: 'Demo Plant' }]}
      />,
    );

    expect(await screen.findByText('调整压力采样周期')).toBeInTheDocument();
    fireEvent.click(screen.getByText('调整压力采样周期').closest('button') as HTMLButtonElement);
    const detail = screen.getByRole('dialog');
    expect(within(detail).getByText('变更差异')).toBeInTheDocument();
    expect(within(detail).getByText('需要实时同步 Runtime', { exact: false })).toBeInTheDocument();

    fireEvent.click(within(detail).getByRole('button', { name: '仿真' }));
    await waitFor(() => expect(onSimulateChangeSet).toHaveBeenCalledWith('change-1'));
    expect(await screen.findByText('仿真预检完成')).toBeInTheDocument();

    fireEvent.click(within(detail).getByRole('button', { name: '确认' }));
    const review = screen.getByRole('heading', { name: '确认执行' }).closest('[role="dialog"]');
    expect(review).not.toBeNull();
    fireEvent.change(within(review as HTMLElement).getByLabelText('治理审核意见'), {
      target: { value: '维护窗口和影响范围已确认' },
    });
    fireEvent.click(within(review as HTMLElement).getByRole('button', { name: '确认候选' }));
    await waitFor(() => expect(onConfirmChangeSet).toHaveBeenCalledWith('change-1', {
      coApprover: null,
      note: '维护窗口和影响范围已确认',
    }));

    fireEvent.click(within(detail).getByRole('button', { name: '应用' }));
    await waitFor(() => expect(onApplyChangeSet).toHaveBeenCalledWith('change-1'));
    expect(await screen.findByText('变更已生效')).toBeInTheDocument();
  });

  it('requires a human note before confirming and dispatching a writable-point command', async () => {
    const onConfirmCommandCandidate = vi.fn().mockResolvedValue(commandCandidate('confirmed'));
    const onDispatchCommandCandidate = vi.fn().mockResolvedValue(commandCandidate('dispatched'));

    render(
      <AgentAssistantPage
        onConfirmCommandCandidate={onConfirmCommandCandidate}
        onDispatchCommandCandidate={onDispatchCommandCandidate}
        onListChangeSets={vi.fn().mockResolvedValue([])}
        onListCommandCandidates={vi.fn().mockResolvedValue([
          commandCandidate('awaiting_confirmation'),
        ])}
        projectOptions={[{ projectId: 'demo-plant', projectName: 'Demo Plant' }]}
      />,
    );

    fireEvent.click(await screen.findByRole('tab', { name: /设备指令/ }));
    expect(await screen.findByText('启动备用泵')).toBeInTheDocument();
    fireEvent.click(screen.getByText('启动备用泵').closest('button') as HTMLButtonElement);
    const detail = screen.getByRole('dialog');
    expect(within(detail).getByText('pump-1 / start_command')).toBeInTheDocument();
    fireEvent.click(within(detail).getByRole('button', { name: '确认' }));

    const review = screen.getByRole('heading', { name: '确认执行' }).closest('[role="dialog"]');
    expect(review).not.toBeNull();
    const confirmButton = within(review as HTMLElement).getByRole('button', { name: '确认候选' });
    expect(confirmButton).toBeDisabled();
    fireEvent.change(within(review as HTMLElement).getByLabelText('治理审核意见'), {
      target: { value: '现场已清场并确认设备处于远程模式' },
    });
    expect(confirmButton).toBeEnabled();
    fireEvent.click(confirmButton);
    await waitFor(() => expect(onConfirmCommandCandidate).toHaveBeenCalledWith('command-1', {
      coApprover: null,
      note: '现场已清场并确认设备处于远程模式',
    }));

    fireEvent.click(within(detail).getByRole('button', { name: '下发' }));
    await waitFor(() => expect(onDispatchCommandCandidate).toHaveBeenCalledWith('command-1'));
    expect(await screen.findByText('指令已下发')).toBeInTheDocument();
  });

  it('shows provider attempts, tool calls, fallback and token usage in the chat trace', async () => {
    render(
      <AgentAssistantPage
        onChat={vi.fn().mockResolvedValue({
          citations: [],
          events: [
            { type: 'started', model: 'edgeops-model' },
            { type: 'provider_attempt', round: 1, attempt: 1 },
            {
              type: 'tool_call_started',
              call: { callId: 'call-1', name: 'read_runtime_metrics', arguments: { edgeId: 'edge-dev' } },
            },
            {
              type: 'tool_call_completed',
              call_id: 'call-1',
              tool_name: 'read_runtime_metrics',
              success: true,
              output: { health: 'healthy' },
            },
            { type: 'security_filtered', source: 'tool:read_runtime_metrics', item_count: 2 },
            { type: 'security_blocked', code: 'agent_input_policy_violation' },
            { type: 'fallback', reason: 'provider timeout' },
            { type: 'completed', mode: 'deterministic', model: 'edgeops-local-analysis' },
          ],
          fallbackReason: 'provider timeout',
          message: '已使用确定性诊断检查 Runtime。',
          mode: 'deterministic',
          model: 'edgeops-local-analysis',
          usage: { completionTokens: 32, promptTokens: 96, totalTokens: 128 },
        })}
      />,
    );

    fireEvent.change(screen.getByLabelText('输入 Agent 问题'), {
      target: { value: '诊断 edge-dev' },
    });
    fireEvent.click(screen.getByRole('button', { name: '发送' }));

    expect(await screen.findByText('执行轨迹', { exact: false })).toBeInTheDocument();
    expect(screen.getAllByText('read_runtime_metrics')).toHaveLength(2);
    expect(screen.getByText('安全策略已拦截')).toBeInTheDocument();
    expect(screen.getByText('已清洗不可信上下文')).toBeInTheDocument();
    expect(screen.getByText('provider timeout')).toBeInTheDocument();
    expect(screen.getByText('128 tokens')).toBeInTheDocument();
  });
});
