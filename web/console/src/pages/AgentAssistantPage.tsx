import { useState } from 'react';
import { ShieldCheck, Sparkles } from 'lucide-react';

const suggestions = [
  ['点位补全', '根据 pump@v1 模型发现缺少 flow_rate 映射', '生成草稿'],
  ['发布风险', 'edge-lab-03 版本落后，建议先单边端灰度', '需确认'],
  ['故障解释', 'pressure 读数中断可能来自 modbus-line-a 超时', '可查看'],
];

export function AgentAssistantPage() {
  const [toolbarMessage, setToolbarMessage] = useState('');

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
            onClick={() => setToolbarMessage('安全策略检查已完成')}
            type="button"
          >
            <ShieldCheck size={15} aria-hidden="true" />
            安全策略
          </button>
          <button
            className="primary-button"
            onClick={() => setToolbarMessage('Agent 建议已生成')}
            type="button"
          >
            <Sparkles size={15} aria-hidden="true" />
            生成建议
          </button>
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>建议队列</h3>
          <span>人工确认后生效</span>
        </div>
        <ul className="detail-list">
          {suggestions.map(([title, detail, state]) => (
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
