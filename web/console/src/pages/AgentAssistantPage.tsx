import { useState } from 'react';
import { ShieldCheck, Sparkles } from 'lucide-react';

import type { AgentActionResponse, AgentSuggestionResponse } from '../api/types';

const fallbackSuggestions: AgentSuggestionResponse[] = [
  {
    detail: '根据 pump@v1 模型发现缺少 flow_rate 映射',
    state: '生成草稿',
    title: '点位补全',
  },
  {
    detail: 'edge-lab-03 版本落后，建议先单边端灰度',
    state: '需确认',
    title: '发布风险',
  },
  {
    detail: 'pressure 读数中断可能来自 modbus-line-a 超时',
    state: '可查看',
    title: '故障解释',
  },
];

export function AgentAssistantPage({
  onGenerateSuggestions,
  onRunSafetyCheck,
}: {
  onGenerateSuggestions?: () => Promise<AgentActionResponse> | AgentActionResponse;
  onRunSafetyCheck?: () => Promise<AgentActionResponse> | AgentActionResponse;
}) {
  const [toolbarMessage, setToolbarMessage] = useState('');
  const [actionState, setActionState] = useState<'idle' | 'checking' | 'generating'>(
    'idle',
  );
  const [suggestions, setSuggestions] =
    useState<AgentSuggestionResponse[]>(fallbackSuggestions);

  const handleRunSafetyCheck = async () => {
    setActionState('checking');
    setToolbarMessage('');

    try {
      const result = await onRunSafetyCheck?.();
      setToolbarMessage(
        result?.status ? `安全策略检查 ${result.status}` : '安全策略检查已完成',
      );
    } catch {
      setToolbarMessage('安全策略检查失败');
    } finally {
      setActionState('idle');
    }
  };

  const handleGenerateSuggestions = async () => {
    setActionState('generating');
    setToolbarMessage('');

    try {
      const result = await onGenerateSuggestions?.();
      if (result?.suggestions && result.suggestions.length > 0) {
        setSuggestions(result.suggestions);
      }
      setToolbarMessage(
        result?.suggestions
          ? `Agent 建议已生成 ${result.suggestions.length} 条`
          : 'Agent 建议已生成',
      );
    } catch {
      setToolbarMessage('Agent 建议生成失败');
    } finally {
      setActionState('idle');
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>Agent 辅助管理</h2>
          <p>
            Agent 用于解释状态、生成配置草稿和风险分析。它不能绕过校验、审批和发布确认。
          </p>
        </div>
        <div className="toolbar">
          {toolbarMessage ? (
            <span className="toolbar-status" role="status">
              {toolbarMessage}
            </span>
          ) : null}
          <button
            className="secondary-button"
            disabled={actionState === 'checking'}
            onClick={() => {
              void handleRunSafetyCheck();
            }}
            type="button"
          >
            <ShieldCheck size={15} aria-hidden="true" />
            {actionState === 'checking' ? '检查中' : '安全策略'}
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
            {actionState === 'generating' ? '生成中' : '生成建议'}
          </button>
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>建议队列</h3>
          <span>人工确认后生效</span>
        </div>
        <ul className="detail-list">
          {suggestions.map(({ detail, state, title }) => (
            <li key={title}>
              <strong>{title}</strong>
              <span>{detail}</span>
              <span className={state === '需确认' ? 'tag warn' : 'tag'}>{state}</span>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
