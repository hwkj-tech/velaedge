import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Activity,
  AlertTriangle,
  BookOpen,
  Bot,
  Check,
  CheckCircle2,
  Clock3,
  FilePlus2,
  GitCompareArrows,
  History,
  MessageSquarePlus,
  Pencil,
  Play,
  RefreshCw,
  Save,
  Send,
  ShieldCheck,
  Sparkles,
  Trash2,
  UserRound,
  X,
} from 'lucide-react';

import type {
  AgentActionResponse,
  AgentChatRequest,
  AgentChatResponse,
  AgentChangeSetResponse,
  AgentChangeSetSimulationResponse,
  AgentCommandCandidateResponse,
  AgentCitationResponse,
  AgentConversationResponse,
  AgentKnowledgeDocumentResponse,
  AgentObservabilityResponse,
  AgentProviderStatusResponse,
  AgentProposalResponse,
  AgentStreamEventResponse,
  AgentSuggestionResponse,
  AgentTokenUsageResponse,
  AgentValidationReportResponse,
  ConfirmAgentGovernanceRequest,
  CreateAgentProposalRequest,
  ReviewAgentProposalRequest,
  RejectAgentGovernanceRequest,
  SaveAgentKnowledgeDocumentRequest,
} from '../api/types';
import { Modal } from '../components/Modal';
import { displayError } from '../utils/errors';

type ChatMessage = {
  body: string;
  id: string;
  role: 'assistant' | 'user';
  citations?: AgentCitationResponse[];
  events?: AgentStreamEventResponse[];
  usage?: AgentTokenUsageResponse;
  fallbackReason?: string;
  suggestions?: AgentSuggestionResponse[];
  title?: string;
};

type GovernanceSelection =
  | { kind: 'change'; item: AgentChangeSetResponse }
  | { kind: 'command'; item: AgentCommandCandidateResponse };

type GovernanceReview = {
  action: 'confirm' | 'reject';
  id: string;
  kind: 'change' | 'command';
  risk: AgentChangeSetResponse['risk'];
  title: string;
};

export function AgentAssistantPage({
  canReviewProposals = true,
  onApplyChangeSet,
  onChat,
  onConfirmChangeSet,
  onConfirmCommandCandidate,
  onCreateProposal,
  onDeleteConversation,
  onDeleteKnowledge,
  onGetMetrics,
  onGetProviderStatus,
  onListKnowledge,
  onListConversations,
  onListChangeSets,
  onListCommandCandidates,
  onGenerateSuggestions,
  onListProposals,
  onReviewProposal,
  onRejectChangeSet,
  onRejectCommandCandidate,
  onRunSafetyCheck,
  onSaveKnowledge,
  onSimulateChangeSet,
  onValidateChangeSet,
  onValidateCommandCandidate,
  onDispatchCommandCandidate,
  projectOptions = [],
}: {
  canReviewProposals?: boolean;
  onApplyChangeSet?: (changeSetId: string) => Promise<AgentChangeSetResponse> | AgentChangeSetResponse;
  onChat?: (request: AgentChatRequest) => Promise<AgentChatResponse> | AgentChatResponse;
  onConfirmChangeSet?: (
    changeSetId: string,
    request: ConfirmAgentGovernanceRequest,
  ) => Promise<AgentChangeSetResponse> | AgentChangeSetResponse;
  onConfirmCommandCandidate?: (
    candidateId: string,
    request: ConfirmAgentGovernanceRequest,
  ) => Promise<AgentCommandCandidateResponse> | AgentCommandCandidateResponse;
  onDeleteConversation?: (conversationId: string) => Promise<void> | void;
  onDeleteKnowledge?: (documentId: string) => Promise<void> | void;
  onCreateProposal?: (
    request: CreateAgentProposalRequest,
  ) => Promise<AgentProposalResponse> | AgentProposalResponse;
  onGenerateSuggestions?: () => Promise<AgentActionResponse> | AgentActionResponse;
  onGetMetrics?: () => Promise<AgentObservabilityResponse> | AgentObservabilityResponse;
  onGetProviderStatus?: () =>
    | Promise<AgentProviderStatusResponse>
    | AgentProviderStatusResponse;
  onListKnowledge?: (
    projectId?: string,
  ) => Promise<AgentKnowledgeDocumentResponse[]> | AgentKnowledgeDocumentResponse[];
  onListConversations?: (
    projectId?: string,
  ) => Promise<AgentConversationResponse[]> | AgentConversationResponse[];
  onListChangeSets?: (
    projectId?: string,
  ) => Promise<AgentChangeSetResponse[]> | AgentChangeSetResponse[];
  onListCommandCandidates?: (
    projectId?: string,
  ) => Promise<AgentCommandCandidateResponse[]> | AgentCommandCandidateResponse[];
  onListProposals?: () => Promise<AgentProposalResponse[]> | AgentProposalResponse[];
  onReviewProposal?: (
    proposalId: string,
    decision: 'approve' | 'reject',
    request: ReviewAgentProposalRequest,
  ) => Promise<AgentProposalResponse> | AgentProposalResponse;
  onRejectChangeSet?: (
    changeSetId: string,
    request: RejectAgentGovernanceRequest,
  ) => Promise<AgentChangeSetResponse> | AgentChangeSetResponse;
  onRejectCommandCandidate?: (
    candidateId: string,
    request: RejectAgentGovernanceRequest,
  ) => Promise<AgentCommandCandidateResponse> | AgentCommandCandidateResponse;
  onRunSafetyCheck?: () => Promise<AgentActionResponse> | AgentActionResponse;
  onSaveKnowledge?: (
    documentId: string | null,
    request: SaveAgentKnowledgeDocumentRequest,
  ) => Promise<AgentKnowledgeDocumentResponse> | AgentKnowledgeDocumentResponse;
  onSimulateChangeSet?: (
    changeSetId: string,
  ) => Promise<AgentChangeSetSimulationResponse> | AgentChangeSetSimulationResponse;
  onValidateChangeSet?: (
    changeSetId: string,
  ) => Promise<AgentChangeSetResponse> | AgentChangeSetResponse;
  onValidateCommandCandidate?: (
    candidateId: string,
  ) => Promise<AgentCommandCandidateResponse> | AgentCommandCandidateResponse;
  onDispatchCommandCandidate?: (
    candidateId: string,
  ) => Promise<AgentCommandCandidateResponse> | AgentCommandCandidateResponse;
  projectOptions?: Array<{ projectId: string; projectName: string }>;
}) {
  const [actionState, setActionState] = useState<
    'idle' | 'checking' | 'generating' | 'chatting'
  >('idle');
  const [draft, setDraft] = useState('');
  const [metrics, setMetrics] = useState<AgentObservabilityResponse>();
  const [provider, setProvider] = useState<AgentProviderStatusResponse>();
  const [selectedProjectId, setSelectedProjectId] = useState(
    projectOptions[0]?.projectId ?? '',
  );
  const [knowledge, setKnowledge] = useState<AgentKnowledgeDocumentResponse[]>([]);
  const [conversations, setConversations] = useState<AgentConversationResponse[]>([]);
  const [activeConversationId, setActiveConversationId] = useState<string>();
  const [conversationAction, setConversationAction] = useState<string>();
  const [pendingConversationDelete, setPendingConversationDelete] = useState(false);
  const [knowledgeEditor, setKnowledgeEditor] = useState<
    AgentKnowledgeDocumentResponse | null | undefined
  >();
  const [knowledgeDraft, setKnowledgeDraft] = useState<SaveAgentKnowledgeDocumentRequest>(
    emptyKnowledgeDraft(projectOptions[0]?.projectId),
  );
  const [knowledgeAction, setKnowledgeAction] = useState<string>();
  const [pendingKnowledgeDelete, setPendingKnowledgeDelete] = useState<string>();
  const [proposalAction, setProposalAction] = useState<string>();
  const [proposals, setProposals] = useState<AgentProposalResponse[]>([]);
  const [changeSets, setChangeSets] = useState<AgentChangeSetResponse[]>([]);
  const [commandCandidates, setCommandCandidates] = useState<AgentCommandCandidateResponse[]>([]);
  const [governanceTab, setGovernanceTab] = useState<'change' | 'command'>('change');
  const [governanceAction, setGovernanceAction] = useState<string>();
  const [governanceSelection, setGovernanceSelection] = useState<GovernanceSelection>();
  const [governanceReview, setGovernanceReview] = useState<GovernanceReview>();
  const [governanceNote, setGovernanceNote] = useState('');
  const [governanceCoApprover, setGovernanceCoApprover] = useState('');
  const [simulations, setSimulations] = useState<Record<string, AgentChangeSetSimulationResponse>>({});
  const [messages, setMessages] = useState<ChatMessage[]>(welcomeMessages());
  const listConversationsRef = useRef(onListConversations);

  useEffect(() => {
    listConversationsRef.current = onListConversations;
  }, [onListConversations]);

  const suggestionCount = useMemo(
    () => messages.reduce((count, message) => count + (message.suggestions?.length ?? 0), 0),
    [messages],
  );
  const activeGovernanceCount = useMemo(
    () =>
      [...changeSets, ...commandCandidates].filter((item) =>
        ['draft', 'awaiting_confirmation', 'confirmed'].includes(item.status),
      ).length,
    [changeSets, commandCandidates],
  );

  const refreshMetrics = useCallback(async () => {
    try {
      const nextMetrics = await onGetMetrics?.();
      if (nextMetrics) setMetrics(nextMetrics);
    } catch {
      // Observability is supplementary and must not interrupt governed workflows.
    }
  }, [onGetMetrics]);

  useEffect(() => {
    let active = true;
    void Promise.resolve(onListProposals?.())
      .then((items) => {
        if (active && items) setProposals(items);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [onListProposals]);

  useEffect(() => {
    let active = true;
    void Promise.all([
      Promise.resolve(onListChangeSets?.(selectedProjectId || undefined)),
      Promise.resolve(onListCommandCandidates?.(selectedProjectId || undefined)),
    ])
      .then(([nextChangeSets, nextCommands]) => {
        if (!active) return;
        if (nextChangeSets) setChangeSets(nextChangeSets);
        if (nextCommands) setCommandCandidates(nextCommands);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [onListChangeSets, onListCommandCandidates, selectedProjectId]);

  useEffect(() => {
    let active = true;
    void Promise.resolve(onGetProviderStatus?.())
      .then((status) => {
        if (active && status) setProvider(status);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [onGetProviderStatus]);

  useEffect(() => {
    void refreshMetrics();
  }, [refreshMetrics]);

  useEffect(() => {
    if (!selectedProjectId && projectOptions[0]?.projectId) {
      setSelectedProjectId(projectOptions[0].projectId);
    }
  }, [projectOptions, selectedProjectId]);

  useEffect(() => {
    let active = true;
    void Promise.resolve(onListKnowledge?.(selectedProjectId || undefined))
      .then((items) => {
        if (active && items) setKnowledge(items);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [onListKnowledge, selectedProjectId]);

  useEffect(() => {
    let active = true;
    setActiveConversationId(undefined);
    setPendingConversationDelete(false);
    setMessages(welcomeMessages());
    void Promise.resolve(
      listConversationsRef.current?.(selectedProjectId || undefined),
    )
      .then((items) => {
        if (active && items) setConversations(items);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [selectedProjectId]);

  const pushMessage = (message: Omit<ChatMessage, 'id'>) => {
    setMessages((current) => [
      ...current,
      {
        ...message,
        id: `message-${Date.now()}-${current.length}`,
      },
    ]);
  };

  const handleRunSafetyCheck = async () => {
    setActionState('checking');
    pushMessage({ body: '请检查当前云边配置的安全策略。', role: 'user' });

    try {
      const result = await onRunSafetyCheck?.();
      pushMessage({
        body: result?.status
          ? `安全策略检查 ${result.status}。当前仍需通过配置校验和发布审批后才能下发到 runtime。`
          : '安全策略检查已完成。当前仍需通过配置校验和发布审批后才能下发到 runtime。',
        role: 'assistant',
        title: '安全策略结果',
      });
    } catch (error) {
      pushMessage({
        body: `安全策略检查失败：${displayError(error)}`,
        role: 'assistant',
        title: '安全策略异常',
      });
    } finally {
      setActionState('idle');
      void refreshMetrics();
    }
  };

  const handleGenerateSuggestions = async () => {
    setActionState('generating');
    pushMessage({ body: '请根据当前边端状态生成候选配置建议。', role: 'user' });

    try {
      const result = await onGenerateSuggestions?.();
      const suggestions = result?.suggestions ?? [];
      pushMessage({
        body: suggestions.length > 0
          ? `已生成 ${suggestions.length} 条候选建议。建议只进入候选队列，不会自动修改配置。`
          : '当前没有可执行的候选建议。',
        role: 'assistant',
        suggestions,
        title: '候选建议',
      });
    } catch (error) {
      pushMessage({
        body: `Agent 建议生成失败：${displayError(error)}`,
        role: 'assistant',
        title: '建议生成异常',
      });
    } finally {
      setActionState('idle');
    }
  };

  const handleSaveProposal = async (suggestion: AgentSuggestionResponse) => {
    const actionId = `save:${suggestion.title}`;
    setProposalAction(actionId);
    try {
      const proposal = await onCreateProposal?.({
        agentId: 'edgeops-agent',
        createdBy: 'console-operator',
        kind: suggestion.title === '点位补全' ? 'point_mapping' : 'config_suggestion',
        payload: { source: 'agent_suggestion' },
        risk: suggestion.state === '需确认' ? 'medium' : 'low',
        summary: suggestion.detail,
        title: suggestion.title,
      });
      if (proposal) {
        setProposals((current) => [proposal, ...current]);
        pushMessage({
          body: `“${suggestion.title}”已保存到审核队列。只有用户应用建议后，变更才会写入并自动同步到 Runtime。`,
          role: 'assistant',
          title: '草案已保存',
        });
      }
    } catch (error) {
      pushMessage({
        body: `保存草案失败：${displayError(error)}`,
        role: 'assistant',
        title: '草案保存异常',
      });
    } finally {
      setProposalAction(undefined);
    }
  };

  const handleReviewProposal = async (
    proposal: AgentProposalResponse,
    decision: 'approve' | 'reject',
  ) => {
    setProposalAction(`${decision}:${proposal.proposalId}`);
    try {
      const reviewed = await onReviewProposal?.(proposal.proposalId, decision, {
        note:
          decision === 'approve'
            ? '允许进入人工配置流程，不自动发布'
            : '需要补充信息后重新提交',
        reviewer: 'console-reviewer',
      });
      if (reviewed) {
        setProposals((current) =>
          current.map((item) =>
            item.proposalId === reviewed.proposalId ? reviewed : item,
          ),
        );
      }
    } catch (error) {
      pushMessage({
        body: `审核草案失败：${displayError(error)}`,
        role: 'assistant',
        title: '审核异常',
      });
    } finally {
      setProposalAction(undefined);
    }
  };

  const replaceChangeSet = (updated: AgentChangeSetResponse) => {
    setChangeSets((current) => upsertById(current, updated, 'changeSetId'));
    setGovernanceSelection((current) =>
      current?.kind === 'change' && current.item.changeSetId === updated.changeSetId
        ? { kind: 'change', item: updated }
        : current,
    );
  };

  const replaceCommandCandidate = (updated: AgentCommandCandidateResponse) => {
    setCommandCandidates((current) => upsertById(current, updated, 'candidateId'));
    setGovernanceSelection((current) =>
      current?.kind === 'command' && current.item.candidateId === updated.candidateId
        ? { kind: 'command', item: updated }
        : current,
    );
  };

  const handleValidateChangeSet = async (item: AgentChangeSetResponse) => {
    setGovernanceAction(`validate-change:${item.changeSetId}`);
    try {
      const updated = await onValidateChangeSet?.(item.changeSetId);
      if (updated) replaceChangeSet(updated);
    } catch (error) {
      pushMessage({
        body: `ChangeSet 校验失败：${displayError(error)}`,
        role: 'assistant',
        title: '变更校验异常',
      });
    } finally {
      setGovernanceAction(undefined);
    }
  };

  const handleSimulateChangeSet = async (item: AgentChangeSetResponse) => {
    setGovernanceAction(`simulate-change:${item.changeSetId}`);
    try {
      const result = await onSimulateChangeSet?.(item.changeSetId);
      if (result) {
        replaceChangeSet(result.changeSet);
        setSimulations((current) => ({ ...current, [item.changeSetId]: result }));
        setGovernanceSelection({ kind: 'change', item: result.changeSet });
      }
    } catch (error) {
      pushMessage({
        body: `ChangeSet 仿真失败：${displayError(error)}`,
        role: 'assistant',
        title: '仿真预检异常',
      });
    } finally {
      setGovernanceAction(undefined);
    }
  };

  const handleValidateCommand = async (item: AgentCommandCandidateResponse) => {
    setGovernanceAction(`validate-command:${item.candidateId}`);
    try {
      const updated = await onValidateCommandCandidate?.(item.candidateId);
      if (updated) replaceCommandCandidate(updated);
    } catch (error) {
      pushMessage({
        body: `设备指令校验失败：${displayError(error)}`,
        role: 'assistant',
        title: '指令校验异常',
      });
    } finally {
      setGovernanceAction(undefined);
    }
  };

  const openGovernanceReview = (
    selection: GovernanceSelection,
    action: GovernanceReview['action'],
  ) => {
    const id = selection.kind === 'change'
      ? selection.item.changeSetId
      : selection.item.candidateId;
    setGovernanceNote('');
    setGovernanceCoApprover('');
    setGovernanceReview({
      action,
      id,
      kind: selection.kind,
      risk: selection.item.risk,
      title: selection.item.title,
    });
  };

  const handleGovernanceReview = async () => {
    if (!governanceReview) return;
    const note = governanceNote.trim();
    if (governanceReview.action === 'reject' && !note) return;
    if (
      governanceReview.kind === 'command' &&
      governanceReview.action === 'confirm' &&
      (!note || (governanceReview.risk === 'critical' && !governanceCoApprover.trim()))
    ) return;

    const actionId = `${governanceReview.action}-${governanceReview.kind}:${governanceReview.id}`;
    setGovernanceAction(actionId);
    try {
      if (governanceReview.kind === 'change') {
        const updated = governanceReview.action === 'confirm'
          ? await onConfirmChangeSet?.(governanceReview.id, {
              coApprover: governanceCoApprover.trim() || null,
              note: note || null,
            })
          : await onRejectChangeSet?.(governanceReview.id, { note });
        if (updated) replaceChangeSet(updated);
      } else {
        const updated = governanceReview.action === 'confirm'
          ? await onConfirmCommandCandidate?.(governanceReview.id, {
              coApprover: governanceCoApprover.trim() || null,
              note: note || null,
            })
          : await onRejectCommandCandidate?.(governanceReview.id, { note });
        if (updated) replaceCommandCandidate(updated);
      }
      setGovernanceReview(undefined);
    } catch (error) {
      pushMessage({
        body: `${governanceReview.action === 'confirm' ? '确认' : '拒绝'}失败：${displayError(error)}`,
        role: 'assistant',
        title: '治理操作异常',
      });
    } finally {
      setGovernanceAction(undefined);
    }
  };

  const handleApplyChangeSet = async (item: AgentChangeSetResponse) => {
    setGovernanceAction(`apply-change:${item.changeSetId}`);
    try {
      const updated = await onApplyChangeSet?.(item.changeSetId);
      if (updated) {
        replaceChangeSet(updated);
        pushMessage({
          body: `“${updated.title}”已应用，受影响边端已收到实时配置同步通知。`,
          role: 'assistant',
          title: '变更已生效',
        });
      }
    } catch (error) {
      pushMessage({
        body: `ChangeSet 应用失败：${displayError(error)}`,
        role: 'assistant',
        title: '变更应用异常',
      });
    } finally {
      setGovernanceAction(undefined);
    }
  };

  const handleDispatchCommand = async (item: AgentCommandCandidateResponse) => {
    setGovernanceAction(`dispatch-command:${item.candidateId}`);
    try {
      const updated = await onDispatchCommandCandidate?.(item.candidateId);
      if (updated) {
        replaceCommandCandidate(updated);
        pushMessage({
          body: `“${updated.title}”已通过 MQTT 下发，Broker 已确认接收。`,
          role: 'assistant',
          title: '指令已下发',
        });
      }
    } catch (error) {
      pushMessage({
        body: `设备指令下发失败：${displayError(error)}`,
        role: 'assistant',
        title: '指令下发异常',
      });
    } finally {
      setGovernanceAction(undefined);
    }
  };

  const openKnowledgeEditor = (document?: AgentKnowledgeDocumentResponse) => {
    setKnowledgeEditor(document ?? null);
    setKnowledgeDraft(
      document
        ? {
            actor: 'console-operator',
            content: document.content,
            enabled: document.enabled,
            projectId: document.projectId,
            sourceUri: document.sourceUri,
            tags: document.tags,
            title: document.title,
          }
        : emptyKnowledgeDraft(selectedProjectId || undefined),
    );
  };

  const handleSaveKnowledge = async () => {
    if (!knowledgeDraft.title.trim() || !knowledgeDraft.content.trim()) return;
    const documentId = knowledgeEditor?.documentId ?? null;
    setKnowledgeAction(`save:${documentId ?? 'new'}`);
    try {
      const saved = await onSaveKnowledge?.(documentId, {
        ...knowledgeDraft,
        projectId: knowledgeDraft.projectId || null,
        tags: knowledgeDraft.tags.map((tag) => tag.trim()).filter(Boolean),
        title: knowledgeDraft.title.trim(),
        content: knowledgeDraft.content.trim(),
      });
      if (saved) {
        setKnowledge((current) => [
          saved,
          ...current.filter((item) => item.documentId !== saved.documentId),
        ]);
        setKnowledgeEditor(undefined);
      }
    } catch (error) {
      pushMessage({
        body: `知识条目保存失败：${displayError(error)}`,
        role: 'assistant',
        title: '知识库异常',
      });
    } finally {
      setKnowledgeAction(undefined);
    }
  };

  const handleDeleteKnowledge = async (documentId: string) => {
    setKnowledgeAction(`delete:${documentId}`);
    try {
      await onDeleteKnowledge?.(documentId);
      setKnowledge((current) =>
        current.filter((item) => item.documentId !== documentId),
      );
      setPendingKnowledgeDelete(undefined);
    } catch (error) {
      pushMessage({
        body: `知识条目删除失败：${displayError(error)}`,
        role: 'assistant',
        title: '知识库异常',
      });
    } finally {
      setKnowledgeAction(undefined);
    }
  };

  const startNewConversation = () => {
    setActiveConversationId(undefined);
    setPendingConversationDelete(false);
    setMessages(welcomeMessages());
  };

  const openConversation = (conversationId: string) => {
    if (!conversationId) {
      startNewConversation();
      return;
    }
    const conversation = conversations.find(
      (item) => item.conversationId === conversationId,
    );
    if (!conversation) return;
    setActiveConversationId(conversationId);
    setPendingConversationDelete(false);
    setMessages(conversationMessages(conversation));
  };

  const handleDeleteConversation = async () => {
    if (!activeConversationId) return;
    setConversationAction(`delete:${activeConversationId}`);
    try {
      await onDeleteConversation?.(activeConversationId);
      setConversations((current) =>
        current.filter((item) => item.conversationId !== activeConversationId),
      );
      startNewConversation();
    } catch (error) {
      pushMessage({
        body: `会话删除失败：${displayError(error)}`,
        role: 'assistant',
        title: '会话管理异常',
      });
    } finally {
      setConversationAction(undefined);
    }
  };

  const handleSend = async () => {
    const text = draft.trim();
    if (!text) return;

    setDraft('');
    setActionState('chatting');
    pushMessage({ body: text, role: 'user' });
    try {
      const response = await onChat?.({
        conversationId: activeConversationId,
        message: text,
        operatorId: 'console-operator',
        projectId: selectedProjectId || undefined,
      });
      pushMessage({
        body:
          response?.message ??
          '当前未连接后端 Agent。请稍后重试，涉及配置变更时仍需保存草案并人工审核。',
        role: 'assistant',
        citations: response?.citations,
        events: response?.events,
        fallbackReason: response?.fallbackReason,
        title: response?.mode === 'openai_compatible' ? '模型分析' : '本地分析',
        usage: response?.usage,
      });
      if (response?.changeSets) {
        setChangeSets((current) =>
          response.changeSets!.reduce(
            (items, item) => upsertById(items, item, 'changeSetId'),
            current,
          ),
        );
      }
      if (response?.commandCandidates) {
        setCommandCandidates((current) =>
          response.commandCandidates!.reduce(
            (items, item) => upsertById(items, item, 'candidateId'),
            current,
          ),
        );
      }
      if (response?.conversationId) {
        setActiveConversationId(response.conversationId);
        void Promise.resolve(
          listConversationsRef.current?.(selectedProjectId || undefined),
        )
          .then((refreshed) => {
            if (refreshed) setConversations(refreshed);
          })
          .catch(() => undefined);
      }
    } catch (error) {
      pushMessage({
        body: `Agent 分析失败：${displayError(error)}`,
        role: 'assistant',
        title: '模型服务异常',
      });
    } finally {
      setActionState('idle');
      void refreshMetrics();
    }
  };

  return (
    <div className="agent-chat-shell">
      <section className="agent-chat-main" aria-label="Agent 对话">
        <div className="agent-chat-hero">
          <div>
            <span>VelaEdge Agent</span>
            <h2>云边智能执行助手</h2>
            <p>诊断跨协议链路，生成可验证变更，并在人工确认后受控执行。</p>
          </div>
          <div className="agent-chat-context">
            <label>
              <span>分析作用域</span>
              <select
                aria-label="Agent 项目作用域"
                onChange={(event) => setSelectedProjectId(event.target.value)}
                value={selectedProjectId}
              >
                <option value="">全局知识</option>
                {projectOptions.map((project) => (
                  <option key={project.projectId} value={project.projectId}>
                    {project.projectName}
                  </option>
                ))}
              </select>
            </label>
            <div className="agent-chat-stats" aria-label="Agent 当前状态">
              <span>待处理事项</span>
              <strong>{activeGovernanceCount || suggestionCount}</strong>
              <small>
                {provider?.mode === 'openai_compatible'
                  ? `${provider.model}${provider.toolCalling ? ' · Tools' : ''}`
                  : '确定性降级可用'}
              </small>
            </div>
          </div>
        </div>

        {metrics ? <AgentObservabilityBar metrics={metrics} /> : null}

        <div className="agent-conversation-toolbar" aria-label="Agent 会话管理">
          <History size={15} aria-hidden="true" />
          <select
            aria-label="Agent 历史会话"
            onChange={(event) => openConversation(event.target.value)}
            value={activeConversationId ?? ''}
          >
            <option value="">新会话</option>
            {conversations.map((conversation) => (
              <option key={conversation.conversationId} value={conversation.conversationId}>
                {conversation.title}
              </option>
            ))}
          </select>
          <button
            aria-label="新建 Agent 会话"
            className="icon-button compact-icon"
            onClick={startNewConversation}
            title="新建会话"
            type="button"
          >
            <MessageSquarePlus size={15} aria-hidden="true" />
          </button>
          {activeConversationId ? (
            pendingConversationDelete ? (
              <div className="agent-conversation-confirm">
                <button
                  aria-label="确认删除当前 Agent 会话"
                  className="icon-button compact-icon danger-icon"
                  disabled={conversationAction === `delete:${activeConversationId}`}
                  onClick={() => void handleDeleteConversation()}
                  title="确认删除"
                  type="button"
                >
                  <Check size={14} aria-hidden="true" />
                </button>
                <button
                  aria-label="取消删除当前 Agent 会话"
                  className="icon-button compact-icon"
                  onClick={() => setPendingConversationDelete(false)}
                  title="取消删除"
                  type="button"
                >
                  <X size={14} aria-hidden="true" />
                </button>
              </div>
            ) : (
              <button
                aria-label="删除当前 Agent 会话"
                className="icon-button compact-icon danger-icon"
                onClick={() => setPendingConversationDelete(true)}
                title="删除当前会话"
                type="button"
              >
                <Trash2 size={15} aria-hidden="true" />
              </button>
            )
          ) : null}
        </div>

        <div className="agent-chat-messages">
          {messages.map((message) => (
            <article className={`agent-message ${message.role}`} key={message.id}>
              <div className="agent-message-avatar">
                {message.role === 'assistant' ? (
                  <Bot size={16} aria-hidden="true" />
                ) : (
                  <UserRound size={16} aria-hidden="true" />
                )}
              </div>
              <div className="agent-message-bubble">
                {message.title ? <strong>{message.title}</strong> : null}
                <p>{message.body}</p>
                {message.suggestions ? (
                  <div className="agent-suggestion-grid">
                    {message.suggestions.map((suggestion) => (
                      <div
                        className="agent-suggestion-card"
                        key={`${message.id}-${suggestion.title}`}
                      >
                        <strong>{suggestion.title}</strong>
                        <span>{suggestion.detail}</span>
                        <div className="agent-suggestion-actions">
                          <small className={suggestion.state === '需确认' ? 'tag warn' : 'tag'}>
                            {suggestion.state}
                          </small>
                          <button
                            aria-label={`保存 ${suggestion.title} 为审核草案`}
                            className="icon-button compact-icon success-icon"
                            disabled={proposalAction === `save:${suggestion.title}`}
                            onClick={() => void handleSaveProposal(suggestion)}
                            title="保存为审核草案"
                            type="button"
                          >
                            <Save size={14} aria-hidden="true" />
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                ) : null}
                {message.events && message.events.length > 0 ? (
                  <AgentToolTimeline events={message.events} />
                ) : null}
                {message.fallbackReason ? (
                  <div className="agent-fallback-note">
                    <AlertTriangle size={14} aria-hidden="true" />
                    模型不可用，已切换确定性分析：{message.fallbackReason}
                  </div>
                ) : null}
                {message.usage ? (
                  <small className="agent-token-usage">
                    {message.usage.totalTokens} tokens
                  </small>
                ) : null}
                {message.citations && message.citations.length > 0 ? (
                  <div className="agent-citations" aria-label="Agent 回答引用">
                    <span>
                      <BookOpen size={13} aria-hidden="true" />
                      引用 {message.citations.length}
                    </span>
                    {message.citations.map((citation, index) => (
                      <article key={`${citation.documentId}:${citation.chunkId ?? index}`}>
                        <strong>{citation.title}</strong>
                        <p>{citation.excerpt}</p>
                        {citation.sourceUri ? <small>{citation.sourceUri}</small> : null}
                      </article>
                    ))}
                  </div>
                ) : null}
              </div>
            </article>
          ))}
        </div>

        <div className="agent-chat-composer">
          <input
            aria-label="输入 Agent 问题"
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                void handleSend();
              }
            }}
            placeholder="询问边端配置风险、点位补全、发布影响..."
            value={draft}
          />
          <button
            className="primary-button"
            disabled={!draft.trim() || actionState === 'chatting'}
            onClick={() => void handleSend()}
            type="button"
          >
            <Send size={15} aria-hidden="true" />
            {actionState === 'chatting' ? '分析中' : '发送'}
          </button>
        </div>
      </section>

      <aside className="agent-chat-side" aria-label="Agent 快捷操作">
        <div>
          <span>快捷动作</span>
          <h3>受控执行</h3>
          <p>只读分析自动运行；变更和设备指令必须校验、确认并完整审计。</p>
        </div>
        <button
          className="secondary-button"
          disabled={actionState === 'checking'}
          onClick={() => {
            void handleRunSafetyCheck();
          }}
          type="button"
        >
          <ShieldCheck size={15} aria-hidden="true" />
          {actionState === 'checking' ? '检查中' : '安全策略检查'}
        </button>
        <button
          className="primary-button"
          disabled={actionState === 'generating'}
          onClick={() => {
            void handleGenerateSuggestions();
          }}
          type="button"
        >
          <Sparkles size={15} aria-hidden="true" />
          {actionState === 'generating' ? '生成中' : '生成候选建议'}
        </button>
        <div className="agent-execution-head">
          <div>
            <span>执行中心</span>
            <strong>{activeGovernanceCount}</strong>
          </div>
          <button
            aria-label="刷新 Agent 执行中心"
            className="icon-button compact-icon"
            disabled={governanceAction === 'refresh'}
            onClick={() => {
              setGovernanceAction('refresh');
              void Promise.all([
                Promise.resolve(onListChangeSets?.(selectedProjectId || undefined)),
                Promise.resolve(onListCommandCandidates?.(selectedProjectId || undefined)),
              ])
                .then(([nextChangeSets, nextCommands]) => {
                  if (nextChangeSets) setChangeSets(nextChangeSets);
                  if (nextCommands) setCommandCandidates(nextCommands);
                })
                .finally(() => setGovernanceAction(undefined));
            }}
            title="刷新执行中心"
            type="button"
          >
            <RefreshCw size={14} aria-hidden="true" />
          </button>
        </div>
        <div className="agent-execution-tabs" role="tablist" aria-label="Agent 执行类型">
          <button
            aria-selected={governanceTab === 'change'}
            className={governanceTab === 'change' ? 'active' : ''}
            onClick={() => setGovernanceTab('change')}
            role="tab"
            type="button"
          >
            配置变更 <span>{changeSets.length}</span>
          </button>
          <button
            aria-selected={governanceTab === 'command'}
            className={governanceTab === 'command' ? 'active' : ''}
            onClick={() => setGovernanceTab('command')}
            role="tab"
            type="button"
          >
            设备指令 <span>{commandCandidates.length}</span>
          </button>
        </div>
        <div className="agent-execution-list">
          {governanceTab === 'change' ? (
            changeSets.length === 0 ? <p>暂无配置变更</p> : changeSets.map((item) => (
              <article key={item.changeSetId}>
                <button
                  className="agent-execution-summary"
                  onClick={() => setGovernanceSelection({ kind: 'change', item })}
                  type="button"
                >
                  <span className={`agent-risk ${item.risk}`}>{riskLabel(item.risk)}</span>
                  <strong>{item.title}</strong>
                  <small>{item.operations.length} 项操作 · {governanceStatusLabel(item.status)}</small>
                </button>
                <GovernanceActions
                  action={governanceAction}
                  canReview={canReviewProposals}
                  item={{ kind: 'change', item }}
                  onApply={() => void handleApplyChangeSet(item)}
                  onConfirm={() => openGovernanceReview({ kind: 'change', item }, 'confirm')}
                  onReject={() => openGovernanceReview({ kind: 'change', item }, 'reject')}
                  onSimulate={() => void handleSimulateChangeSet(item)}
                  onValidate={() => void handleValidateChangeSet(item)}
                />
              </article>
            ))
          ) : commandCandidates.length === 0 ? <p>暂无设备指令候选</p> : commandCandidates.map((item) => (
            <article key={item.candidateId}>
              <button
                className="agent-execution-summary"
                onClick={() => setGovernanceSelection({ kind: 'command', item })}
                type="button"
              >
                <span className={`agent-risk ${item.risk}`}>{riskLabel(item.risk)}</span>
                <strong>{item.title}</strong>
                <small>{item.target.edgeId} · {item.target.pointId} = {formatValue(item.value)}</small>
              </button>
              <GovernanceActions
                action={governanceAction}
                canReview={canReviewProposals}
                item={{ kind: 'command', item }}
                onConfirm={() => openGovernanceReview({ kind: 'command', item }, 'confirm')}
                onDispatch={() => void handleDispatchCommand(item)}
                onReject={() => openGovernanceReview({ kind: 'command', item }, 'reject')}
                onValidate={() => void handleValidateCommand(item)}
              />
            </article>
          ))}
        </div>
        <div className="agent-governance-head">
          <span>兼容建议草案</span>
          <strong>{proposals.filter((item) => item.status === 'pending_review').length}</strong>
        </div>
        <div className="agent-governance-list">
          {proposals.length === 0 ? (
            <p>暂无已保存草案</p>
          ) : (
            proposals.map((proposal) => (
              <article key={proposal.proposalId}>
                <div>
                  <strong>{proposal.title}</strong>
                  <small className={`tag ${proposal.status === 'rejected' ? 'warn' : ''}`}>
                    {proposalStatusLabel(proposal.status)}
                  </small>
                </div>
                <p>{proposal.summary}</p>
                {proposal.status === 'pending_review' && canReviewProposals ? (
                  <div className="agent-review-actions">
                    <button
                      aria-label={`通过 ${proposal.title}`}
                      className="icon-button compact-icon success-icon"
                      disabled={Boolean(proposalAction)}
                      onClick={() => void handleReviewProposal(proposal, 'approve')}
                      title="通过草案"
                      type="button"
                    >
                      <Check size={14} aria-hidden="true" />
                    </button>
                    <button
                      aria-label={`驳回 ${proposal.title}`}
                      className="icon-button compact-icon danger-icon"
                      disabled={Boolean(proposalAction)}
                      onClick={() => void handleReviewProposal(proposal, 'reject')}
                      title="驳回草案"
                      type="button"
                    >
                      <X size={14} aria-hidden="true" />
                    </button>
                  </div>
                ) : proposal.status === 'pending_review' ? (
                  <small>需要管理员审核</small>
                ) : (
                  <small>{proposal.reviewedBy ?? '未知审核人'}</small>
                )}
              </article>
            ))
          )}
        </div>
        <div className="agent-knowledge-head">
          <div>
            <span>受管知识</span>
            <strong>{knowledge.filter((item) => item.enabled).length}</strong>
          </div>
          <button
            aria-label="新增知识条目"
            className="icon-button compact-icon"
            onClick={() => openKnowledgeEditor()}
            title="新增知识条目"
            type="button"
          >
            <FilePlus2 size={15} aria-hidden="true" />
          </button>
        </div>
        <div className="agent-knowledge-list">
          {knowledge.length === 0 ? (
            <p>当前作用域暂无知识条目</p>
          ) : (
            knowledge.map((document) => (
              <article key={document.documentId}>
                <div>
                  <strong>{document.title}</strong>
                  <small className={document.enabled ? 'tag' : 'tag warn'}>
                    {document.enabled ? '启用' : '停用'}
                  </small>
                </div>
                <p>{document.tags.join(' · ') || '未设置标签'}</p>
                <div className="agent-knowledge-actions">
                  <button
                    aria-label={`编辑知识 ${document.title}`}
                    className="icon-button compact-icon"
                    onClick={() => openKnowledgeEditor(document)}
                    title="编辑知识条目"
                    type="button"
                  >
                    <Pencil size={13} aria-hidden="true" />
                  </button>
                  {pendingKnowledgeDelete === document.documentId ? (
                    <>
                      <button
                        aria-label={`确认删除知识 ${document.title}`}
                        className="icon-button compact-icon danger-icon"
                        disabled={knowledgeAction === `delete:${document.documentId}`}
                        onClick={() => void handleDeleteKnowledge(document.documentId)}
                        title="确认删除"
                        type="button"
                      >
                        <Check size={13} aria-hidden="true" />
                      </button>
                      <button
                        aria-label={`取消删除知识 ${document.title}`}
                        className="icon-button compact-icon"
                        onClick={() => setPendingKnowledgeDelete(undefined)}
                        title="取消删除"
                        type="button"
                      >
                        <X size={13} aria-hidden="true" />
                      </button>
                    </>
                  ) : (
                    <button
                      aria-label={`删除知识 ${document.title}`}
                      className="icon-button compact-icon danger-icon"
                      onClick={() => setPendingKnowledgeDelete(document.documentId)}
                      title="删除知识条目"
                      type="button"
                    >
                      <Trash2 size={13} aria-hidden="true" />
                    </button>
                  )}
                </div>
              </article>
            ))
          )}
        </div>
      </aside>
      {governanceSelection ? (
        <Modal onClose={() => setGovernanceSelection(undefined)}>
          <section className="modal-panel agent-governance-modal" role="dialog" aria-modal="true">
            <header className="modal-header">
              <div>
                <span>{governanceSelection.kind === 'change' ? 'ChangeSet' : 'Device Command'}</span>
                <h2>{governanceSelection.item.title}</h2>
                <p>{governanceSelection.item.rationale}</p>
              </div>
              <button
                aria-label="关闭执行详情"
                className="icon-button"
                onClick={() => setGovernanceSelection(undefined)}
                type="button"
              >
                <X size={18} aria-hidden="true" />
              </button>
            </header>
            <div className="modal-body agent-governance-detail">
              <div className="agent-governance-facts">
                <span className={`agent-risk ${governanceSelection.item.risk}`}>
                  {riskLabel(governanceSelection.item.risk)}
                </span>
                <span>{governanceStatusLabel(governanceSelection.item.status)}</span>
                <span>{formatTimestamp(governanceSelection.item.updatedAt)}</span>
              </div>
              {governanceSelection.kind === 'change' ? (
                <>
                  <section>
                    <div className="agent-detail-heading">
                      <GitCompareArrows size={17} aria-hidden="true" />
                      <div><strong>变更差异</strong><span>{governanceSelection.item.operations.length} 项确定性操作</span></div>
                    </div>
                    <div className="agent-operation-list">
                      {governanceSelection.item.operations.map((operation) => (
                        <article key={operation.operationId}>
                          <span className={`agent-operation-kind ${operation.kind}`}>{operation.kind}</span>
                          <div>
                            <strong>{operation.resourceKind} / {operation.resourceId}</strong>
                            <small>{operation.operationId}</small>
                          </div>
                          <details>
                            <summary>查看 JSON 差异</summary>
                            <div className="agent-json-diff">
                              <pre>{JSON.stringify(operation.before ?? null, null, 2)}</pre>
                              <pre>{JSON.stringify(operation.after ?? null, null, 2)}</pre>
                            </div>
                          </details>
                        </article>
                      ))}
                    </div>
                  </section>
                  <ValidationPanel report={governanceSelection.item.validation} />
                  {simulations[governanceSelection.item.changeSetId] ? (
                    <section className="agent-simulation-result">
                      <div className="agent-detail-heading">
                        <Play size={17} aria-hidden="true" />
                        <div>
                          <strong>仿真预检完成</strong>
                          <span>
                            已生成 {simulations[governanceSelection.item.changeSetId].packages.length} 个 Runtime 配置包，未写入生产状态
                          </span>
                        </div>
                      </div>
                    </section>
                  ) : null}
                </>
              ) : (
                <>
                  <section className="agent-command-target">
                    <div><span>边端</span><strong>{governanceSelection.item.target.edgeId}</strong></div>
                    <div><span>设备 / 点位</span><strong>{governanceSelection.item.target.deviceId} / {governanceSelection.item.target.pointId}</strong></div>
                    <div><span>写入值</span><strong>{formatValue(governanceSelection.item.value)}</strong></div>
                    <div><span>幂等键</span><strong>{governanceSelection.item.idempotencyKey}</strong></div>
                  </section>
                  <ValidationPanel report={governanceSelection.item.validation} />
                </>
              )}
            </div>
            <footer className="modal-actions">
              <GovernanceActions
                action={governanceAction}
                canReview={canReviewProposals}
                item={governanceSelection}
                onApply={governanceSelection.kind === 'change' ? () => void handleApplyChangeSet(governanceSelection.item) : undefined}
                onConfirm={() => openGovernanceReview(governanceSelection, 'confirm')}
                onDispatch={governanceSelection.kind === 'command' ? () => void handleDispatchCommand(governanceSelection.item) : undefined}
                onReject={() => openGovernanceReview(governanceSelection, 'reject')}
                onSimulate={governanceSelection.kind === 'change' ? () => void handleSimulateChangeSet(governanceSelection.item) : undefined}
                onValidate={governanceSelection.kind === 'change'
                  ? () => void handleValidateChangeSet(governanceSelection.item)
                  : () => void handleValidateCommand(governanceSelection.item)}
              />
              <button className="secondary-button" onClick={() => setGovernanceSelection(undefined)} type="button">
                关闭
              </button>
            </footer>
          </section>
        </Modal>
      ) : null}
      {governanceReview ? (
        <Modal onClose={() => setGovernanceReview(undefined)}>
          <section className="modal-panel agent-review-modal" role="dialog" aria-modal="true">
            <header className="modal-header">
              <div>
                <span>Human Governance</span>
                <h2>{governanceReview.action === 'confirm' ? '确认执行' : '拒绝候选'}</h2>
                <p>{governanceReview.title} · {riskLabel(governanceReview.risk)}</p>
              </div>
              <button aria-label="关闭确认弹窗" className="icon-button" onClick={() => setGovernanceReview(undefined)} type="button">
                <X size={18} aria-hidden="true" />
              </button>
            </header>
            <div className="modal-body agent-review-form">
              <div className="agent-review-warning">
                <ShieldCheck size={18} aria-hidden="true" />
                <div>
                  <strong>模型不能替你批准</strong>
                  <p>本次操作将记录当前登录人、确认意见、校验结果与后续执行结果。</p>
                </div>
              </div>
              <label>
                <span>{governanceReview.action === 'reject' || governanceReview.kind === 'command' ? '审核意见（必填）' : '审核意见'}</span>
                <textarea
                  aria-label="治理审核意见"
                  onChange={(event) => setGovernanceNote(event.target.value)}
                  placeholder="说明现场确认、影响窗口或拒绝原因"
                  rows={4}
                  value={governanceNote}
                />
              </label>
              {governanceReview.action === 'confirm' && governanceReview.risk === 'critical' ? (
                <label>
                  <span>第二审批人（必填）</span>
                  <input
                    aria-label="第二审批人"
                    onChange={(event) => setGovernanceCoApprover(event.target.value)}
                    placeholder="输入另一位管理员标识"
                    value={governanceCoApprover}
                  />
                </label>
              ) : null}
            </div>
            <footer className="modal-actions">
              <button className="secondary-button" onClick={() => setGovernanceReview(undefined)} type="button">取消</button>
              <button
                className={governanceReview.action === 'reject' ? 'danger-button' : 'primary-button'}
                disabled={
                  Boolean(governanceAction) ||
                  ((governanceReview.action === 'reject' || governanceReview.kind === 'command') && !governanceNote.trim()) ||
                  (governanceReview.action === 'confirm' && governanceReview.risk === 'critical' && !governanceCoApprover.trim())
                }
                onClick={() => void handleGovernanceReview()}
                type="button"
              >
                {governanceReview.action === 'confirm' ? <CheckCircle2 size={15} aria-hidden="true" /> : <X size={15} aria-hidden="true" />}
                {governanceReview.action === 'confirm' ? '确认候选' : '拒绝候选'}
              </button>
            </footer>
          </section>
        </Modal>
      ) : null}
      {knowledgeEditor !== undefined ? (
        <Modal onClose={() => setKnowledgeEditor(undefined)}>
          <section className="modal-panel agent-knowledge-modal" role="dialog" aria-modal="true">
            <header className="modal-header">
              <div>
                <span>Agent Knowledge</span>
                <h2>{knowledgeEditor ? '编辑知识条目' : '新增知识条目'}</h2>
                <p>内容仅用于受限检索和回答引用，不会触发配置同步或设备指令。</p>
              </div>
              <button
                aria-label="关闭知识编辑弹窗"
                className="icon-button"
                onClick={() => setKnowledgeEditor(undefined)}
                type="button"
              >
                <X size={18} aria-hidden="true" />
              </button>
            </header>
            <div className="modal-body agent-knowledge-form">
              <label>
                <span>标题</span>
                <input
                  aria-label="知识标题"
                  onChange={(event) =>
                    setKnowledgeDraft((current) => ({
                      ...current,
                      title: event.target.value,
                    }))
                  }
                  value={knowledgeDraft.title}
                />
              </label>
              <div className="form-grid two-columns">
                <label>
                  <span>项目作用域</span>
                  <select
                    aria-label="知识项目作用域"
                    onChange={(event) =>
                      setKnowledgeDraft((current) => ({
                        ...current,
                        projectId: event.target.value || null,
                      }))
                    }
                    value={knowledgeDraft.projectId ?? ''}
                  >
                    <option value="">全局共享</option>
                    {projectOptions.map((project) => (
                      <option key={project.projectId} value={project.projectId}>
                        {project.projectName}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  <span>来源标识</span>
                  <input
                    aria-label="知识来源标识"
                    onChange={(event) =>
                      setKnowledgeDraft((current) => ({
                        ...current,
                        sourceUri: event.target.value,
                      }))
                    }
                    placeholder="kb://manual/modbus"
                    value={knowledgeDraft.sourceUri ?? ''}
                  />
                </label>
              </div>
              <label>
                <span>标签</span>
                <input
                  aria-label="知识标签"
                  onChange={(event) =>
                    setKnowledgeDraft((current) => ({
                      ...current,
                      tags: event.target.value.split(','),
                    }))
                  }
                  placeholder="Modbus, 运维, 超时"
                  value={knowledgeDraft.tags.join(', ')}
                />
              </label>
              <label>
                <span>知识正文</span>
                <textarea
                  aria-label="知识正文"
                  onChange={(event) =>
                    setKnowledgeDraft((current) => ({
                      ...current,
                      content: event.target.value,
                    }))
                  }
                  rows={12}
                  value={knowledgeDraft.content}
                />
              </label>
              <label className="toggle-row">
                <input
                  aria-label="启用知识条目"
                  checked={knowledgeDraft.enabled}
                  onChange={(event) =>
                    setKnowledgeDraft((current) => ({
                      ...current,
                      enabled: event.target.checked,
                    }))
                  }
                  type="checkbox"
                />
                <span>启用检索</span>
              </label>
            </div>
            <footer className="modal-actions">
              <button
                className="secondary-button"
                onClick={() => setKnowledgeEditor(undefined)}
                type="button"
              >
                取消
              </button>
              <button
                className="primary-button"
                disabled={
                  !knowledgeDraft.title.trim() ||
                  !knowledgeDraft.content.trim() ||
                  knowledgeAction?.startsWith('save:')
                }
                onClick={() => void handleSaveKnowledge()}
                type="button"
              >
                <Save size={15} aria-hidden="true" />
                保存
              </button>
            </footer>
          </section>
        </Modal>
      ) : null}
    </div>
  );
}

function AgentToolTimeline({ events }: { events: AgentStreamEventResponse[] }) {
  const visibleEvents = events.filter((event) =>
    [
      'provider_attempt',
      'tool_call_started',
      'tool_call_completed',
      'security_blocked',
      'security_filtered',
      'fallback',
      'completed',
    ].includes(event.type),
  );
  if (visibleEvents.length === 0) return null;

  return (
    <details className="agent-tool-timeline">
      <summary>
        <Activity size={14} aria-hidden="true" />
        执行轨迹 <span>{visibleEvents.filter((event) => event.type === 'tool_call_completed').length} 个工具</span>
      </summary>
      <ol>
        {visibleEvents.map((event, index) => (
          <li className={event.type} key={`${event.type}-${index}`}>
            {event.type === 'provider_attempt' ? (
              <><Clock3 size={13} aria-hidden="true" /><span>模型调用</span><strong>第 {event.round} 轮 / 第 {event.attempt} 次</strong></>
            ) : event.type === 'tool_call_started' ? (
              <><Activity size={13} aria-hidden="true" /><span>调用工具</span><strong>{event.call.name}</strong></>
            ) : event.type === 'tool_call_completed' ? (
              <>{event.success ? <CheckCircle2 size={13} aria-hidden="true" /> : <AlertTriangle size={13} aria-hidden="true" />}<span>{event.success ? '工具完成' : '工具失败'}</span><strong>{event.tool_name}</strong></>
            ) : event.type === 'security_blocked' ? (
              <><ShieldCheck size={13} aria-hidden="true" /><span>安全策略已拦截</span><strong>{event.code}</strong></>
            ) : event.type === 'security_filtered' ? (
              <><ShieldCheck size={13} aria-hidden="true" /><span>已清洗不可信上下文</span><strong>{event.source} · {event.item_count} 项</strong></>
            ) : event.type === 'fallback' ? (
              <><AlertTriangle size={13} aria-hidden="true" /><span>确定性降级</span><strong>{event.reason}</strong></>
            ) : event.type === 'completed' ? (
              <><CheckCircle2 size={13} aria-hidden="true" /><span>分析完成</span><strong>{event.model}</strong></>
            ) : null}
          </li>
        ))}
      </ol>
    </details>
  );
}

function AgentObservabilityBar({ metrics }: { metrics: AgentObservabilityResponse }) {
  const items = [
    ['请求', formatCompactNumber(metrics.requestCount)],
    ['工具', `${formatCompactNumber(metrics.toolCallCount)} / ${metrics.toolFailureCount} 失败`],
    ['降级', formatCompactNumber(metrics.fallbackCount)],
    ['安全', `${formatCompactNumber(metrics.securityBlockCount)} 拦截 · ${formatCompactNumber(metrics.securityFilterCount)} 清洗`],
    ['平均延迟', `${formatCompactNumber(metrics.averageLatencyMs)} ms`],
    ['Token', formatCompactNumber(metrics.totalTokens)],
    ['估算成本', formatAgentCost(metrics.estimatedCostMicrousd)],
  ] as const;

  return (
    <div
      className="agent-observability-bar"
      aria-label="Agent 真实运行指标"
      title={`Provider 尝试 ${metrics.providerAttemptCount} 次，成功 ${metrics.providerSuccessCount} 次，失败请求 ${metrics.failedRequestCount} 次`}
    >
      {items.map(([label, value]) => (
        <span key={label}>
          <small>{label}</small>
          <strong>{value}</strong>
        </span>
      ))}
    </div>
  );
}

function formatCompactNumber(value: number): string {
  return new Intl.NumberFormat('zh-CN', {
    maximumFractionDigits: value >= 1000 ? 1 : 0,
    notation: value >= 1000 ? 'compact' : 'standard',
  }).format(value);
}

function formatAgentCost(microUsd: number): string {
  if (microUsd === 0) return '$0';
  const usd = microUsd / 1_000_000;
  return `$${usd < 0.01 ? usd.toFixed(4) : usd.toFixed(2)}`;
}

function GovernanceActions({
  action,
  canReview,
  item,
  onApply,
  onConfirm,
  onDispatch,
  onReject,
  onSimulate,
  onValidate,
}: {
  action?: string;
  canReview: boolean;
  item: GovernanceSelection;
  onApply?: () => void;
  onConfirm: () => void;
  onDispatch?: () => void;
  onReject: () => void;
  onSimulate?: () => void;
  onValidate: () => void;
}) {
  const status = item.item.status;
  const busy = Boolean(action);
  if (['applied', 'dispatched', 'rejected', 'failed'].includes(status)) {
    return <small className={`agent-terminal-state ${status}`}>{governanceStatusLabel(status)}</small>;
  }

  return (
    <div className="agent-execution-actions">
      {status === 'draft' ? (
        <button className="secondary-button" disabled={busy} onClick={onValidate} type="button">
          <ShieldCheck size={13} aria-hidden="true" />校验
        </button>
      ) : null}
      {status === 'awaiting_confirmation' && item.kind === 'change' && onSimulate ? (
        <button className="secondary-button" disabled={busy} onClick={onSimulate} type="button">
          <Play size={13} aria-hidden="true" />仿真
        </button>
      ) : null}
      {status === 'awaiting_confirmation' && canReview ? (
        <>
          <button className="primary-button" disabled={busy} onClick={onConfirm} type="button">
            <Check size={13} aria-hidden="true" />确认
          </button>
          <button aria-label={`拒绝 ${item.item.title}`} className="icon-button compact-icon danger-icon" disabled={busy} onClick={onReject} title="拒绝候选" type="button">
            <X size={13} aria-hidden="true" />
          </button>
        </>
      ) : status === 'awaiting_confirmation' ? <small>需要管理员确认</small> : null}
      {status === 'confirmed' && canReview && item.kind === 'change' && onApply ? (
        <button className="primary-button" disabled={busy} onClick={onApply} type="button">
          <GitCompareArrows size={13} aria-hidden="true" />应用
        </button>
      ) : null}
      {status === 'confirmed' && canReview && item.kind === 'command' && onDispatch ? (
        <button className="primary-button" disabled={busy} onClick={onDispatch} type="button">
          <Send size={13} aria-hidden="true" />下发
        </button>
      ) : null}
      {status === 'confirmed' && !canReview ? <small>需要管理员执行</small> : null}
    </div>
  );
}

function ValidationPanel({ report }: { report?: AgentValidationReportResponse | null }) {
  if (!report) {
    return (
      <section className="agent-validation-panel pending">
        <Clock3 size={17} aria-hidden="true" />
        <div><strong>等待校验</strong><span>尚未生成拓扑、配置和策略校验报告</span></div>
      </section>
    );
  }
  return (
    <section className={`agent-validation-panel ${report.valid ? 'valid' : 'invalid'}`}>
      {report.valid ? <CheckCircle2 size={17} aria-hidden="true" /> : <AlertTriangle size={17} aria-hidden="true" />}
      <div>
        <strong>{report.valid ? '校验通过' : '校验未通过'}</strong>
        <span>
          影响 {report.impact.affectedResources} 个资源 · {report.impact.affectedEdges.length} 个边端
          {report.impact.requiresRuntimeSync ? ' · 需要实时同步 Runtime' : ''}
        </span>
        {report.issues.length > 0 ? (
          <ul>
            {report.issues.map((issue) => (
              <li key={`${issue.code}:${issue.path}`}><b>{issue.code}</b>{issue.message}</li>
            ))}
          </ul>
        ) : null}
      </div>
    </section>
  );
}

function upsertById<T, K extends keyof T>(items: T[], item: T, key: K): T[] {
  return [item, ...items.filter((current) => current[key] !== item[key])];
}

function riskLabel(risk: AgentChangeSetResponse['risk']) {
  switch (risk) {
    case 'low': return '低风险';
    case 'medium': return '中风险';
    case 'high': return '高风险';
    case 'critical': return '关键风险';
  }
}

function governanceStatusLabel(status: AgentChangeSetResponse['status'] | AgentCommandCandidateResponse['status']) {
  switch (status) {
    case 'draft': return '待校验';
    case 'awaiting_confirmation': return '待确认';
    case 'confirmed': return '已确认';
    case 'applying': return '应用中';
    case 'applied': return '已应用';
    case 'dispatching': return '下发中';
    case 'dispatched': return '已下发';
    case 'rejected': return '已拒绝';
    case 'failed': return '失败';
  }
}

function formatValue(value: unknown) {
  return typeof value === 'string' ? value : JSON.stringify(value);
}

function formatTimestamp(value: string) {
  const timestamp = Date.parse(value);
  return Number.isNaN(timestamp) ? value : new Intl.DateTimeFormat('zh-CN', {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(timestamp);
}

function emptyKnowledgeDraft(projectId?: string): SaveAgentKnowledgeDocumentRequest {
  return {
    actor: 'console-operator',
    content: '',
    enabled: true,
    projectId: projectId ?? null,
    sourceUri: null,
    tags: [],
    title: '',
  };
}

function welcomeMessages(): ChatMessage[] {
  return [
    {
      body: '我可以帮你检查边端配置风险、生成候选点位和解释发布影响。所有建议都需要人工确认后才会生效。',
      id: 'welcome',
      role: 'assistant',
      title: 'Agent 助手已就绪',
    },
  ];
}

function conversationMessages(conversation: AgentConversationResponse): ChatMessage[] {
  return conversation.messages.map((message) => ({
    body: message.content,
    citations: message.citations,
    id: message.messageId,
    role: message.role,
    title: message.role === 'assistant' ? '历史分析' : undefined,
  }));
}

function proposalStatusLabel(status: AgentProposalResponse['status']) {
  switch (status) {
    case 'pending_review':
      return '待审核';
    case 'approved':
      return '已通过';
    case 'rejected':
      return '已驳回';
  }
}
