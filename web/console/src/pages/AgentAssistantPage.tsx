import { useEffect, useMemo, useRef, useState } from 'react';
import {
  BookOpen,
  Bot,
  Check,
  FilePlus2,
  History,
  MessageSquarePlus,
  Pencil,
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
  AgentCitationResponse,
  AgentConversationResponse,
  AgentKnowledgeDocumentResponse,
  AgentProviderStatusResponse,
  AgentProposalResponse,
  AgentSuggestionResponse,
  CreateAgentProposalRequest,
  ReviewAgentProposalRequest,
  SaveAgentKnowledgeDocumentRequest,
} from '../api/types';
import { Modal } from '../components/Modal';
import { displayError } from '../utils/errors';

type ChatMessage = {
  body: string;
  id: string;
  role: 'assistant' | 'user';
  citations?: AgentCitationResponse[];
  suggestions?: AgentSuggestionResponse[];
  title?: string;
};

export function AgentAssistantPage({
  canReviewProposals = true,
  onChat,
  onCreateProposal,
  onDeleteConversation,
  onDeleteKnowledge,
  onGetProviderStatus,
  onListKnowledge,
  onListConversations,
  onGenerateSuggestions,
  onListProposals,
  onReviewProposal,
  onRunSafetyCheck,
  onSaveKnowledge,
  projectOptions = [],
}: {
  canReviewProposals?: boolean;
  onChat?: (request: AgentChatRequest) => Promise<AgentChatResponse> | AgentChatResponse;
  onDeleteConversation?: (conversationId: string) => Promise<void> | void;
  onDeleteKnowledge?: (documentId: string) => Promise<void> | void;
  onCreateProposal?: (
    request: CreateAgentProposalRequest,
  ) => Promise<AgentProposalResponse> | AgentProposalResponse;
  onGenerateSuggestions?: () => Promise<AgentActionResponse> | AgentActionResponse;
  onGetProviderStatus?: () =>
    | Promise<AgentProviderStatusResponse>
    | AgentProviderStatusResponse;
  onListKnowledge?: (
    projectId?: string,
  ) => Promise<AgentKnowledgeDocumentResponse[]> | AgentKnowledgeDocumentResponse[];
  onListConversations?: (
    projectId?: string,
  ) => Promise<AgentConversationResponse[]> | AgentConversationResponse[];
  onListProposals?: () => Promise<AgentProposalResponse[]> | AgentProposalResponse[];
  onReviewProposal?: (
    proposalId: string,
    decision: 'approve' | 'reject',
    request: ReviewAgentProposalRequest,
  ) => Promise<AgentProposalResponse> | AgentProposalResponse;
  onRunSafetyCheck?: () => Promise<AgentActionResponse> | AgentActionResponse;
  onSaveKnowledge?: (
    documentId: string | null,
    request: SaveAgentKnowledgeDocumentRequest,
  ) => Promise<AgentKnowledgeDocumentResponse> | AgentKnowledgeDocumentResponse;
  projectOptions?: Array<{ projectId: string; projectName: string }>;
}) {
  const [actionState, setActionState] = useState<
    'idle' | 'checking' | 'generating' | 'chatting'
  >('idle');
  const [draft, setDraft] = useState('');
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
  const [messages, setMessages] = useState<ChatMessage[]>(welcomeMessages());
  const listConversationsRef = useRef(onListConversations);

  useEffect(() => {
    listConversationsRef.current = onListConversations;
  }, [onListConversations]);

  const suggestionCount = useMemo(
    () => messages.reduce((count, message) => count + (message.suggestions?.length ?? 0), 0),
    [messages],
  );

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
          body: `“${suggestion.title}”已保存到审核队列。保存和审核都不会自动发布配置。`,
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
        title: response?.mode === 'openai_compatible' ? '模型分析' : '本地分析',
      });
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
    }
  };

  return (
    <div className="agent-chat-shell">
      <section className="agent-chat-main" aria-label="Agent 对话">
        <div className="agent-chat-hero">
          <div>
            <span>VelaEdge Agent</span>
            <h2>云边配置助手</h2>
            <p>用对话方式分析运行状态、配置风险、候选点位和发布影响。</p>
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
              <span>候选建议</span>
              <strong>{suggestionCount}</strong>
              <small>
                {provider?.mode === 'openai_compatible'
                  ? provider.model
                  : '本地受控分析'}
              </small>
            </div>
          </div>
        </div>

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
                  disabled={conversationAction === `delete:${activeConversationId}`}
                  onClick={() => void handleDeleteConversation()}
                  title="确认删除"
                  type="button"
                >
                  <Check size={14} aria-hidden="true" />
                </button>
                <button
                  aria-label="取消删除当前 Agent 会话"
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
                {message.citations && message.citations.length > 0 ? (
                  <div className="agent-citations" aria-label="Agent 回答引用">
                    <span>
                      <BookOpen size={13} aria-hidden="true" />
                      引用 {message.citations.length}
                    </span>
                    {message.citations.map((citation) => (
                      <article key={citation.documentId}>
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
          <p>Agent 只生成解释和候选建议，不会绕过校验、审批和发布确认。</p>
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
        <div className="agent-governance-head">
          <span>审核队列</span>
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
                      disabled={Boolean(proposalAction)}
                      onClick={() => void handleReviewProposal(proposal, 'approve')}
                      title="通过草案"
                      type="button"
                    >
                      <Check size={14} aria-hidden="true" />
                    </button>
                    <button
                      aria-label={`驳回 ${proposal.title}`}
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
                        disabled={knowledgeAction === `delete:${document.documentId}`}
                        onClick={() => void handleDeleteKnowledge(document.documentId)}
                        title="确认删除"
                        type="button"
                      >
                        <Check size={13} aria-hidden="true" />
                      </button>
                      <button
                        aria-label={`取消删除知识 ${document.title}`}
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
      {knowledgeEditor !== undefined ? (
        <Modal onClose={() => setKnowledgeEditor(undefined)}>
          <section className="modal-panel agent-knowledge-modal" role="dialog" aria-modal="true">
            <header className="modal-header">
              <div>
                <span>Agent Knowledge</span>
                <h2>{knowledgeEditor ? '编辑知识条目' : '新增知识条目'}</h2>
                <p>内容仅用于受限检索和回答引用，不会触发配置发布或设备指令。</p>
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
