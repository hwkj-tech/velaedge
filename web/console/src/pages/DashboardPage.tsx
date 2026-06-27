import { useState } from 'react';
import { Send, Wrench } from 'lucide-react';

import type { PointMappingResponse, SummaryResponse } from '../api/types';

const edgeHealth = [
  ['edge-shanghai-01', '上海一厂', '在线', 'v2026.06.26-001', 'v2026.06.26-001', '18 秒前'],
  ['edge-suzhou-02', '苏州测试线', '在线', 'v2026.06.26-001', 'v2026.06.26-001', '24 秒前'],
  ['edge-lab-03', '研发实验室', '离线', 'v2026.06.26-002', 'v2026.06.25-004', '11 分钟前'],
];

const events = [
  ['配置校验完成', '点位 pressure、temperature 已通过边端能力检查'],
  ['边端上报延迟', 'edge-lab-03 最近一次心跳超过 10 分钟'],
  ['算法告警', 'pump-anomaly-v1 对 pump-1 产生高风险告警'],
];

export function DashboardPage({
  loadState,
  onCreatePoint,
  onPublish,
  summary,
}: {
  loadState: 'loading' | 'ready' | 'error';
  onCreatePoint?: () => Promise<PointMappingResponse> | PointMappingResponse;
  onPublish?: () => Promise<void> | void;
  summary: SummaryResponse;
}) {
  const onlineRate = summary.edge_count > 0 ? '66.7%' : '--';
  const [toolbarMessage, setToolbarMessage] = useState('');
  const [actionState, setActionState] = useState<
    'idle' | 'creating-point' | 'publishing'
  >('idle');

  const handleCreatePoint = async () => {
    setActionState('creating-point');
    setToolbarMessage('');

    try {
      const created = await onCreatePoint?.();
      setToolbarMessage(
        created ? `已创建点位草稿 ${created.pointId}` : '已创建点位配置草稿',
      );
    } catch {
      setToolbarMessage('创建点位失败');
    } finally {
      setActionState('idle');
    }
  };

  const handlePublish = async () => {
    setActionState('publishing');
    setToolbarMessage('');

    try {
      await onPublish?.();
      setToolbarMessage('已创建发布，等待 runtime 回报');
    } catch {
      setToolbarMessage('发布配置失败');
    } finally {
      setActionState('idle');
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>运行总览</h2>
          <p>
            汇总边端在线状态、配置草稿、发布进度和采集异常，帮助运维人员快速进入待处理工作。
          </p>
        </div>
        <div className="toolbar" aria-label="快捷操作">
          {toolbarMessage ? (
            <span className="toolbar-status" role="status">
              {toolbarMessage}
            </span>
          ) : null}
          <span className="toolbar-status">边端连接后自动发现</span>
          <button
            className="secondary-button"
            disabled={actionState === 'creating-point'}
            onClick={() => {
              void handleCreatePoint();
            }}
            type="button"
          >
            <Wrench size={15} aria-hidden="true" />
            {actionState === 'creating-point' ? '创建中' : '创建点位'}
          </button>
          <button
            className="primary-button"
            disabled={actionState === 'publishing'}
            onClick={() => {
              void handlePublish();
            }}
            type="button"
          >
            <Send size={15} aria-hidden="true" />
            {actionState === 'publishing' ? '发布中' : '发布配置'}
          </button>
        </div>
      </section>

      <section className="metric-grid" aria-label="工作台指标">
        <Metric label="边端节点" value={String(summary.edge_count)} hint="云端已注册" />
        <Metric label="在线率" value={onlineRate} hint="最近 60 秒心跳" />
        <Metric label="遥测点位" value="128" hint="启用 116 个" />
        <Metric label="异常点位" value="4" hint="需处理" tone="alert" />
        <Metric
          label="待发布"
          value={String(summary.pending_release_count)}
          hint="草稿配置"
        />
        <Metric label="高风险告警" value="2" hint="Agent 已标记" tone="alert" />
      </section>

      <div className="dashboard-grid">
        <section className="panel" aria-labelledby="edge-health-title">
          <div className="panel-header">
            <h3 id="edge-health-title">边端健康状态</h3>
            <span>{loadState === 'ready' ? 'API 已连接' : '样例数据'}</span>
          </div>
          <div className="table-wrap">
            <table className="ops-table">
              <thead>
                <tr>
                  <th>Edge ID</th>
                  <th>站点</th>
                  <th>状态</th>
                  <th>期望版本</th>
                  <th>上报版本</th>
                  <th>心跳</th>
                </tr>
              </thead>
              <tbody>
                {edgeHealth.map(([edgeId, site, status, desired, reported, heartbeat]) => (
                  <tr key={edgeId}>
                    <td>{edgeId}</td>
                    <td>{site}</td>
                    <td>
                      <span className={status === '在线' ? 'tag ok' : 'tag danger'}>
                        {status}
                      </span>
                    </td>
                    <td>{desired}</td>
                    <td>{reported}</td>
                    <td>{heartbeat}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>

        <section className="panel" aria-labelledby="events-title">
          <div className="panel-header">
            <h3 id="events-title">最近事件</h3>
            <span>实时审计流</span>
          </div>
          <ul className="timeline-list">
            {events.map(([title, description]) => (
              <li key={title}>
                <strong>{title}</strong>
                <span>{description}</span>
              </li>
            ))}
          </ul>
        </section>
      </div>
    </div>
  );
}

function Metric({
  hint,
  label,
  tone,
  value,
}: {
  hint: string;
  label: string;
  tone?: 'alert';
  value: string;
}) {
  return (
    <article className={tone === 'alert' ? 'metric-tile alert' : 'metric-tile'}>
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{hint}</small>
    </article>
  );
}
