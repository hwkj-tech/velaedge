import {
  Activity,
  ArrowRight,
  Bot,
  CheckCircle2,
  ClipboardCheck,
  Eye,
  FileDiff,
  KeyRound,
  Network,
  Play,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
  TerminalSquare,
  XCircle,
} from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';

import type {
  AgentChangeSetResponse,
  AgentChangeSetSimulationResponse,
  AgentCommandCandidateResponse,
  AuditRecordResponse,
  ConfirmAgentGovernanceRequest,
  McpStatusResponse,
  RejectAgentGovernanceRequest,
} from '../api/types';
import { Modal } from '../components/Modal';

type IntegrationTab = 'capabilities' | 'approvals' | 'audit';
type ApprovalKind = 'change' | 'command';
type Selection =
  | { kind: 'change'; item: AgentChangeSetResponse }
  | { kind: 'command'; item: AgentCommandCandidateResponse };
type ReviewAction = 'confirm' | 'reject';

interface AiIntegrationPageProps {
  canExecute?: boolean;
  onGetStatus?: () => Promise<McpStatusResponse>;
  onListChangeSets?: (projectId?: string) => Promise<AgentChangeSetResponse[]>;
  onListCommandCandidates?: (projectId?: string) => Promise<AgentCommandCandidateResponse[]>;
  onListAudit?: () => Promise<AuditRecordResponse[]>;
  onValidateChangeSet?: (id: string) => Promise<AgentChangeSetResponse>;
  onSimulateChangeSet?: (id: string) => Promise<AgentChangeSetSimulationResponse>;
  onConfirmChangeSet?: (
    id: string,
    request: ConfirmAgentGovernanceRequest,
  ) => Promise<AgentChangeSetResponse>;
  onRejectChangeSet?: (
    id: string,
    request: RejectAgentGovernanceRequest,
  ) => Promise<AgentChangeSetResponse>;
  onApplyChangeSet?: (id: string) => Promise<AgentChangeSetResponse>;
  onValidateCommandCandidate?: (id: string) => Promise<AgentCommandCandidateResponse>;
  onConfirmCommandCandidate?: (
    id: string,
    request: ConfirmAgentGovernanceRequest,
  ) => Promise<AgentCommandCandidateResponse>;
  onRejectCommandCandidate?: (
    id: string,
    request: RejectAgentGovernanceRequest,
  ) => Promise<AgentCommandCandidateResponse>;
  onDispatchCommandCandidate?: (id: string) => Promise<AgentCommandCandidateResponse>;
}

export function AiIntegrationPage({
  canExecute = false,
  onGetStatus,
  onListChangeSets,
  onListCommandCandidates,
  onListAudit,
  onValidateChangeSet,
  onSimulateChangeSet,
  onConfirmChangeSet,
  onRejectChangeSet,
  onApplyChangeSet,
  onValidateCommandCandidate,
  onConfirmCommandCandidate,
  onRejectCommandCandidate,
  onDispatchCommandCandidate,
}: AiIntegrationPageProps) {
  const [tab, setTab] = useState<IntegrationTab>('capabilities');
  const [approvalKind, setApprovalKind] = useState<ApprovalKind>('change');
  const [status, setStatus] = useState<McpStatusResponse>();
  const [changeSets, setChangeSets] = useState<AgentChangeSetResponse[]>([]);
  const [commands, setCommands] = useState<AgentCommandCandidateResponse[]>([]);
  const [audit, setAudit] = useState<AuditRecordResponse[]>([]);
  const [selection, setSelection] = useState<Selection>();
  const [review, setReview] = useState<ReviewAction>();
  const [note, setNote] = useState('');
  const [coApprover, setCoApprover] = useState('');
  const [busy, setBusy] = useState<string>();
  const [error, setError] = useState<string>();
  const [simulation, setSimulation] = useState<AgentChangeSetSimulationResponse>();

  const refresh = useCallback(async () => {
    setBusy('refresh');
    setError(undefined);
    try {
      const [nextStatus, nextChanges, nextCommands, nextAudit] = await Promise.all([
        onGetStatus?.(),
        onListChangeSets?.(),
        onListCommandCandidates?.(),
        onListAudit?.(),
      ]);
      if (nextStatus) setStatus(nextStatus);
      if (nextChanges) setChangeSets(nextChanges);
      if (nextCommands) setCommands(nextCommands);
      if (nextAudit) setAudit(nextAudit);
    } catch (cause) {
      setError(displayError(cause));
    } finally {
      setBusy(undefined);
    }
  }, [onGetStatus, onListAudit, onListChangeSets, onListCommandCandidates]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const pendingChanges = changeSets.filter((item) => isPending(item.status)).length;
  const pendingCommands = commands.filter((item) => isPending(item.status)).length;
  const mcpAudit = useMemo(
    () => audit.filter((item) => item.action === 'invoke_agent_tool' || item.target.startsWith('mcp-tool:')),
    [audit],
  );

  const replaceChange = (updated: AgentChangeSetResponse) => {
    setChangeSets((items) => upsert(items, updated, 'changeSetId'));
    setSelection((current) =>
      current?.kind === 'change' && current.item.changeSetId === updated.changeSetId
        ? { kind: 'change', item: updated }
        : current,
    );
  };

  const replaceCommand = (updated: AgentCommandCandidateResponse) => {
    setCommands((items) => upsert(items, updated, 'candidateId'));
    setSelection((current) =>
      current?.kind === 'command' && current.item.candidateId === updated.candidateId
        ? { kind: 'command', item: updated }
        : current,
    );
  };

  const run = async (key: string, action: () => Promise<void>) => {
    setBusy(key);
    setError(undefined);
    try {
      await action();
      const nextAudit = await onListAudit?.();
      if (nextAudit) setAudit(nextAudit);
    } catch (cause) {
      setError(displayError(cause));
    } finally {
      setBusy(undefined);
    }
  };

  const validateSelection = () => {
    if (!selection) return;
    if (selection.kind === 'change') {
      void run(`validate:${selection.item.changeSetId}`, async () => {
        const updated = await onValidateChangeSet?.(selection.item.changeSetId);
        if (updated) replaceChange(updated);
      });
    } else {
      void run(`validate:${selection.item.candidateId}`, async () => {
        const updated = await onValidateCommandCandidate?.(selection.item.candidateId);
        if (updated) replaceCommand(updated);
      });
    }
  };

  const simulateChange = () => {
    if (selection?.kind !== 'change') return;
    void run(`simulate:${selection.item.changeSetId}`, async () => {
      const result = await onSimulateChangeSet?.(selection.item.changeSetId);
      if (result) {
        setSimulation(result);
        replaceChange(result.changeSet);
      }
    });
  };

  const executeConfirmed = () => {
    if (!selection) return;
    if (selection.kind === 'change') {
      void run(`apply:${selection.item.changeSetId}`, async () => {
        const updated = await onApplyChangeSet?.(selection.item.changeSetId);
        if (updated) replaceChange(updated);
      });
    } else {
      void run(`dispatch:${selection.item.candidateId}`, async () => {
        const updated = await onDispatchCommandCandidate?.(selection.item.candidateId);
        if (updated) replaceCommand(updated);
      });
    }
  };

  const submitReview = () => {
    if (!selection || !review) return;
    const id = selection.kind === 'change' ? selection.item.changeSetId : selection.item.candidateId;
    const key = `${review}:${id}`;
    void run(key, async () => {
      if (selection.kind === 'change') {
        const updated = review === 'confirm'
          ? await onConfirmChangeSet?.(id, { note: note.trim() || null, coApprover: coApprover.trim() || null })
          : await onRejectChangeSet?.(id, { note: note.trim() });
        if (updated) replaceChange(updated);
      } else {
        const updated = review === 'confirm'
          ? await onConfirmCommandCandidate?.(id, { note: note.trim() || null, coApprover: coApprover.trim() || null })
          : await onRejectCommandCandidate?.(id, { note: note.trim() });
        if (updated) replaceCommand(updated);
      }
      setReview(undefined);
      setNote('');
      setCoApprover('');
    });
  };

  return (
    <section className="ai-integration-page">
      <header className="ai-integration-heading">
        <div>
          <span>MODEL CONTEXT PROTOCOL</span>
          <h2>AI 集成治理</h2>
          <p>向外部 Agent 开放受控能力，所有配置变更与设备指令先进入人工审批。</p>
        </div>
        <div className="ai-integration-heading-actions">
          <span className={status?.enabled ? 'status-badge success' : 'status-badge warning'}>
            <span className="status-dot" /> {status?.enabled ? 'MCP 在线' : 'MCP 未启用'}
          </span>
          <button className="icon-command" disabled={busy === 'refresh'} onClick={() => void refresh()} title="刷新" type="button">
            <RefreshCw aria-hidden="true" size={16} />
          </button>
        </div>
      </header>

      <nav aria-label="AI 集成视图" className="ai-integration-tabs">
        <button className={tab === 'capabilities' ? 'active' : ''} onClick={() => setTab('capabilities')} type="button">
          <Network size={15} /> 能力目录
        </button>
        <button className={tab === 'approvals' ? 'active' : ''} onClick={() => setTab('approvals')} type="button">
          <ClipboardCheck size={15} /> 审批中心 <span>{pendingChanges + pendingCommands}</span>
        </button>
        <button className={tab === 'audit' ? 'active' : ''} onClick={() => setTab('audit')} type="button">
          <Activity size={15} /> 调用审计
        </button>
      </nav>

      {error ? <div className="ai-inline-error"><XCircle size={16} /> {error}</div> : null}

      {tab === 'capabilities' ? <Capabilities status={status} /> : null}
      {tab === 'approvals' ? (
        <Approvals
          approvalKind={approvalKind}
          changeSets={changeSets}
          commands={commands}
          onKindChange={setApprovalKind}
          onSelect={setSelection}
        />
      ) : null}
      {tab === 'audit' ? <McpAudit rows={mcpAudit} /> : null}

      {selection ? (
        <ApprovalDetail
          busy={busy}
          canExecute={canExecute}
          onClose={() => { setSelection(undefined); setSimulation(undefined); }}
          onExecute={executeConfirmed}
          onReview={(action) => { setReview(action); setNote(''); setCoApprover(''); }}
          onSimulate={simulateChange}
          onValidate={validateSelection}
          selection={selection}
          simulation={simulation}
        />
      ) : null}

      {selection && review ? (
        <Modal onClose={() => setReview(undefined)}>
          <section aria-label={review === 'confirm' ? '确认审批' : '拒绝审批'} className="modal-panel compact-modal ai-review-modal">
            <header className="modal-header">
              <div>
                <h3>{review === 'confirm' ? '确认审批' : '拒绝候选'}</h3>
                <p>{selection.item.title}</p>
              </div>
              <button aria-label="关闭" className="modal-close" onClick={() => setReview(undefined)} type="button">×</button>
            </header>
            <div className="ai-review-form">
              <label>审核说明<textarea onChange={(event) => setNote(event.target.value)} value={note} /></label>
              {review === 'confirm' && selection.item.risk === 'critical' ? (
                <label>第二审批人<input onChange={(event) => setCoApprover(event.target.value)} value={coApprover} /></label>
              ) : null}
            </div>
            <footer className="modal-actions">
              <span>{review === 'confirm' ? '确认不会立即绕过执行权限' : '拒绝后保留完整审计记录'}</span>
              <div>
                <button className="secondary" onClick={() => setReview(undefined)} type="button">取消</button>
                <button
                  className={review === 'confirm' ? 'primary' : 'danger'}
                  disabled={(review === 'reject' && !note.trim()) || (selection.item.risk === 'critical' && review === 'confirm' && !coApprover.trim()) || Boolean(busy)}
                  onClick={submitReview}
                  type="button"
                >
                  {review === 'confirm' ? '确认' : '拒绝'}
                </button>
              </div>
            </footer>
          </section>
        </Modal>
      ) : null}
    </section>
  );
}

function Capabilities({ status }: { status?: McpStatusResponse }) {
  if (!status) return <div className="ai-empty"><RotateCcw size={24} /><strong>正在读取 MCP 状态</strong></div>;
  return (
    <div className="ai-capability-view">
      <section className="ai-endpoint-band">
        <div><TerminalSquare size={18} /><span>服务端点</span><strong>{status.endpoint}</strong></div>
        <div><Network size={18} /><span>传输</span><strong>Streamable HTTP</strong></div>
        <div><KeyRound size={18} /><span>鉴权</span><strong>{status.authentication.required ? 'Bearer Token' : '本地开发模式'}</strong></div>
        <div><ShieldCheck size={18} /><span>执行边界</span><strong>只读 + 候选草案</strong></div>
      </section>
      <section className="ai-policy-strip">
        <span><CheckCircle2 size={15} /> Origin 校验</span>
        <span><CheckCircle2 size={15} /> 全量审计</span>
        <span><CheckCircle2 size={15} /> {status.controls.rateLimitPerMinute} 次/分钟</span>
        <span><CheckCircle2 size={15} /> 不开放直接执行</span>
        <small>MCP {status.protocolVersion} · Server {status.serverVersion}</small>
      </section>
      <section className="ai-table-section">
        <header><div><h3>工具目录</h3><p>当前账号可见的模型工具</p></div><strong>{status.tools.length} 个工具</strong></header>
        <div className="table-scroll">
          <table className="ai-table">
            <thead><tr><th>工具</th><th>能力说明</th><th>影响</th><th>风险</th><th>人工审批</th></tr></thead>
            <tbody>{status.tools.map((tool) => (
              <tr key={tool.name}>
                <td><code>{tool.name}</code></td><td>{tool.description}</td>
                <td><span className={tool.readOnly ? 'status-badge neutral' : 'status-badge warning'}>{tool.readOnly ? '只读' : '生成候选'}</span></td>
                <td><Risk risk={tool.risk} /></td><td>{tool.humanReviewRequired ? '必须' : '不需要'}</td>
              </tr>
            ))}</tbody>
          </table>
        </div>
      </section>
    </div>
  );
}

function Approvals({ approvalKind, changeSets, commands, onKindChange, onSelect }: {
  approvalKind: ApprovalKind;
  changeSets: AgentChangeSetResponse[];
  commands: AgentCommandCandidateResponse[];
  onKindChange: (kind: ApprovalKind) => void;
  onSelect: (selection: Selection) => void;
}) {
  return (
    <section className="ai-table-section">
      <header>
        <div><h3>候选审批</h3><p>外部 Agent 只能把操作送到这里，不能直接修改 Runtime 或写入设备</p></div>
        <div className="ai-segmented">
          <button className={approvalKind === 'change' ? 'active' : ''} onClick={() => onKindChange('change')} type="button">配置变更 {changeSets.length}</button>
          <button className={approvalKind === 'command' ? 'active' : ''} onClick={() => onKindChange('command')} type="button">设备指令 {commands.length}</button>
        </div>
      </header>
      <div className="table-scroll">
        {approvalKind === 'change' ? (
          <table className="ai-table"><thead><tr><th>候选</th><th>目标</th><th>风险</th><th>校验</th><th>状态</th><th>更新时间</th><th>操作</th></tr></thead>
            <tbody>{changeSets.map((item) => <tr key={item.changeSetId}><td><strong>{item.title}</strong><code>{item.changeSetId}</code></td><td>{item.target.productId ?? item.target.projectId}</td><td><Risk risk={item.risk} /></td><td>{item.validation?.valid ? '通过' : '待校验'}</td><td><Status status={item.status} /></td><td>{formatTime(item.updatedAt)}</td><td><button className="table-action" onClick={() => onSelect({ kind: 'change', item })} type="button"><Eye size={14} /> 查看</button></td></tr>)}</tbody>
          </table>
        ) : (
          <table className="ai-table"><thead><tr><th>候选</th><th>边端 / 点位</th><th>风险</th><th>校验</th><th>状态</th><th>更新时间</th><th>操作</th></tr></thead>
            <tbody>{commands.map((item) => <tr key={item.candidateId}><td><strong>{item.title}</strong><code>{item.candidateId}</code></td><td>{item.target.edgeId} / {item.target.pointId}</td><td><Risk risk={item.risk} /></td><td>{item.validation?.valid ? '通过' : '待校验'}</td><td><Status status={item.status} /></td><td>{formatTime(item.updatedAt)}</td><td><button className="table-action" onClick={() => onSelect({ kind: 'command', item })} type="button"><Eye size={14} /> 查看</button></td></tr>)}</tbody>
          </table>
        )}
      </div>
      {(approvalKind === 'change' ? changeSets : commands).length === 0 ? <div className="ai-empty"><ClipboardCheck size={24} /><strong>暂无候选审批</strong></div> : null}
    </section>
  );
}

function McpAudit({ rows }: { rows: AuditRecordResponse[] }) {
  return <section className="ai-table-section"><header><div><h3>MCP 调用审计</h3><p>记录调用主体、工具、作用域和执行结果</p></div><strong>{rows.length} 条记录</strong></header><div className="table-scroll"><table className="ai-table"><thead><tr><th>时间</th><th>主体</th><th>工具与结果</th><th>审计动作</th><th>状态</th></tr></thead><tbody>{rows.map((row, index) => <tr key={`${row.createdAt}-${index}`}><td>{formatTime(row.createdAt)}</td><td>{row.actor}</td><td><code>{row.target}</code></td><td>{row.action}</td><td><span className="status-badge success">{row.result}</span></td></tr>)}</tbody></table></div>{rows.length === 0 ? <div className="ai-empty"><Activity size={24} /><strong>暂无 MCP 调用记录</strong></div> : null}</section>;
}

function ApprovalDetail({ selection, simulation, busy, canExecute, onClose, onValidate, onSimulate, onReview, onExecute }: {
  selection: Selection;
  simulation?: AgentChangeSetSimulationResponse;
  busy?: string;
  canExecute: boolean;
  onClose: () => void;
  onValidate: () => void;
  onSimulate: () => void;
  onReview: (action: ReviewAction) => void;
  onExecute: () => void;
}) {
  const item = selection.item;
  const id = selection.kind === 'change'
    ? selection.item.changeSetId
    : selection.item.candidateId;
  const confirmed = item.status === 'confirmed';
  const awaiting = item.status === 'draft' || item.status === 'awaiting_confirmation';
  return <Modal onClose={onClose}><section aria-label="候选详情" className="modal-panel ai-approval-modal"><header className="modal-header"><div><h3>{selection.kind === 'change' ? '配置变更候选' : '设备指令候选'}</h3><p>{item.title} · {id}</p></div><button aria-label="关闭" className="modal-close" onClick={onClose} type="button">×</button></header><div className="ai-approval-body"><section className="ai-detail-summary"><div><span>状态</span><Status status={item.status} /></div><div><span>风险</span><Risk risk={item.risk} /></div><div><span>创建人</span><strong>{item.createdBy}</strong></div><div><span>校验</span><strong>{item.validation?.valid ? '通过' : '未通过或待校验'}</strong></div></section><section className="ai-detail-section"><h4>变更理由</h4><p>{item.rationale}</p></section>{selection.kind === 'change' ? <><section className="ai-detail-section"><h4>资源影响</h4><div className="ai-operation-list">{selection.item.operations.map((operation) => <div key={operation.operationId}><FileDiff size={15} /><strong>{operation.kind}</strong><span>{operation.resourceKind}</span><code>{operation.resourceId}</code></div>)}</div></section>{simulation ? <section className="ai-detail-section"><h4>仿真结果</h4><p>生成 {simulation.packages.length} 个配置包，未写入任何 Runtime。</p></section> : null}</> : <section className="ai-detail-section"><h4>指令目标</h4><div className="ai-command-target"><code>{selection.item.target.edgeId}</code><ArrowRight size={14} /><code>{selection.item.target.protocolConnectionId}</code><ArrowRight size={14} /><code>{selection.item.target.deviceId}.{selection.item.target.pointId}</code></div><pre>{JSON.stringify(selection.item.value, null, 2)}</pre></section>}{item.validation?.issues.length ? <section className="ai-detail-section"><h4>校验问题</h4>{item.validation.issues.map((issue) => <p key={`${issue.code}-${issue.path}`}>{issue.severity}: {issue.message}</p>)}</section> : null}</div><footer className="modal-actions"><span>{canExecute ? '审批与执行由独立权限控制' : '当前账号仅可查看候选'}</span><div>{canExecute && awaiting ? <><button className="secondary" disabled={Boolean(busy)} onClick={onValidate} type="button"><ShieldCheck size={14} /> 校验</button>{selection.kind === 'change' ? <button className="secondary" disabled={Boolean(busy)} onClick={onSimulate} type="button"><Play size={14} /> 仿真</button> : null}<button className="danger" disabled={Boolean(busy)} onClick={() => onReview('reject')} type="button">拒绝</button><button className="primary" disabled={Boolean(busy) || !item.validation?.valid} onClick={() => onReview('confirm')} type="button"><CheckCircle2 size={14} /> 确认</button></> : null}{canExecute && confirmed ? <button className="primary" disabled={Boolean(busy)} onClick={onExecute} type="button"><Bot size={14} /> {selection.kind === 'change' ? '应用并同步' : '下发指令'}</button> : null}<button className="secondary" onClick={onClose} type="button">关闭</button></div></footer></section></Modal>;
}

function Risk({ risk }: { risk: string }) { return <span className={`ai-risk ${risk}`}>{({ low: '低', medium: '中', high: '高', critical: '严重' } as Record<string, string>)[risk] ?? risk}</span>; }
function Status({ status }: { status: string }) { return <span className={`status-badge ${['applied', 'dispatched'].includes(status) ? 'success' : ['rejected', 'failed'].includes(status) ? 'danger' : 'warning'}`}>{statusLabel(status)}</span>; }
function statusLabel(status: string) { return ({ draft: '草案', awaiting_confirmation: '待确认', confirmed: '已确认', applying: '应用中', applied: '已应用', dispatching: '下发中', dispatched: '已下发', rejected: '已拒绝', failed: '失败' } as Record<string, string>)[status] ?? status; }
function isPending(status: string) { return ['draft', 'awaiting_confirmation', 'confirmed'].includes(status); }
function formatTime(value: string) { const date = new Date(value); return Number.isNaN(date.getTime()) ? value : date.toLocaleString('zh-CN', { hour12: false }); }
function displayError(cause: unknown) { return cause instanceof Error ? cause.message : String(cause); }
function upsert<T extends Record<K, string>, K extends keyof T>(items: T[], updated: T, key: K) { return [updated, ...items.filter((item) => item[key] !== updated[key])]; }
