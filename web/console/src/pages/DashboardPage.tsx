import type {
  AuditRecordResponse,
  EdgeNodeResponse,
  EdgeRuntimeMetricsSnapshot,
  RuntimeStatusResponse,
  SummaryResponse,
} from '../api/types';

export function DashboardPage({
  auditRecords = [],
  edgeNodes = [],
  loadState,
  runtimeStatus,
  summary,
}: {
  auditRecords?: AuditRecordResponse[];
  edgeNodes?: EdgeNodeResponse[];
  loadState: 'loading' | 'ready' | 'error';
  runtimeStatus?: RuntimeStatusResponse;
  summary: SummaryResponse;
}) {
  const runtimeEdges = runtimeStatus?.edges ?? [];
  const onlineRate = formatPercent(
    summary.edge_count > 0
      ? ((runtimeStatus?.healthyEdgeCount ?? healthyEdgeCount(edgeNodes)) /
          summary.edge_count) *
          100
      : undefined,
  );
  const activeTaskCount = runtimeEdges.reduce(
    (total, edge) => total + edge.collection.active_task_count,
    0,
  );
  const badPointCount = runtimeEdges.reduce(
    (total, edge) => total + edge.collection.bad_point_count,
    0,
  );
  const bufferedRecords = runtimeEdges.reduce(
    (total, edge) => total + edge.local_store.buffered_records,
    0,
  );
  const healthyCount = runtimeStatus?.healthyEdgeCount ?? healthyEdgeCount(edgeNodes);
  const issueCount =
    (runtimeStatus?.degradedEdgeCount ?? 0) + (runtimeStatus?.criticalEdgeCount ?? 0);
  const averageSuccessRate = runtimeEdges.length
    ? runtimeEdges.reduce((total, edge) => total + edge.collection.success_rate, 0) /
      runtimeEdges.length
    : undefined;
  const syncLag = runtimeEdges.length
    ? Math.max(...runtimeEdges.map((edge) => edge.cloud_sync.last_sync_seconds_ago))
    : undefined;

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>Dashboard</h2>
          <p>
            只展示边端在线、系统资源、采集质量、同步延迟和最近事件；配置入口统一放到各管理页面。
          </p>
        </div>
        <div className="status-strip" aria-label="Dashboard 数据状态">
          <span className={loadState === 'ready' ? 'status-pill online' : 'status-pill'}>
            {loadState === 'ready' ? '监控 API 已连接' : '监控数据加载中'}
          </span>
        </div>
      </section>

      <section className="command-overview" aria-label="EdgeOps 指挥态势">
        <div className="command-overview-main">
          <span>EdgeOps 指挥态势</span>
          <strong>{onlineRate}</strong>
          <p>
            {healthyCount} 个 runtime 健康在线
            {issueCount > 0 ? `，${issueCount} 个边端需要处理` : '，采集链路运行稳定'}。
          </p>
        </div>
        <div className="command-overview-rail">
          <CommandSignal
            label="数据链路"
            value={averageSuccessRate === undefined ? '--' : formatPercent(averageSuccessRate * 100)}
            hint={`平均延迟 ${runtimeStatus ? `${runtimeStatus.averageCollectionLatencyMs}ms` : '--'}`}
          />
          <CommandSignal
            label="待发布配置"
            value={String(summary.pending_release_count)}
            hint={summary.pending_release_count > 0 ? '等待校验下发' : '版本已对齐'}
            tone={summary.pending_release_count > 0 ? 'warning' : undefined}
          />
          <CommandSignal
            label="runtime 同步"
            value={syncLag === undefined ? '--' : `${syncLag}s`}
            hint="最近一次云边握手"
          />
          <CommandSignal
            label="RocksDB 缓存"
            value={String(bufferedRecords)}
            hint={bufferedRecords > 0 ? '等待 MQTT 上报' : '无积压'}
            tone={bufferedRecords > 0 ? 'warning' : undefined}
          />
        </div>
      </section>

      <section className="metric-grid" aria-label="Dashboard 指标">
        <Metric label="边端节点" value={String(summary.edge_count)} hint="云端已登记" />
        <Metric label="在线率" value={onlineRate} hint="按 runtime 健康状态计算" />
        <Metric
          label="平均延迟"
          value={runtimeStatus ? `${runtimeStatus.averageCollectionLatencyMs}ms` : '--'}
          hint="采集链路平均值"
        />
        <Metric label="运行任务" value={String(activeTaskCount)} hint="边端当前任务数" />
        <Metric
          label="异常点位"
          value={String(badPointCount)}
          hint="runtime 上报质量异常"
          tone={badPointCount > 0 ? 'alert' : undefined}
        />
        <Metric
          label="缓存积压"
          value={String(bufferedRecords)}
          hint="本地存储待上传记录"
          tone={bufferedRecords > 0 ? 'alert' : undefined}
        />
      </section>

      <div className="dashboard-grid">
        <section className="panel" aria-labelledby="edge-monitor-title">
          <div className="panel-header">
            <h3 id="edge-monitor-title">边端运行监控</h3>
            <span>{runtimeEdges.length || edgeNodes.length} 个边端</span>
          </div>
          <div className="table-wrap">
            <table className="ops-table">
              <thead>
                <tr>
                  <th>Edge ID</th>
                  <th>Runtime</th>
                  <th>健康</th>
                  <th>CPU / 内存 / 磁盘</th>
                  <th>任务 / 异常点</th>
                  <th>期望 / 上报版本</th>
                  <th>同步</th>
                </tr>
              </thead>
              <tbody>
                {runtimeEdges.length > 0
                  ? runtimeEdges.map((edge) => (
                      <RuntimeEdgeRow edge={edge} key={edge.edge_id} />
                    ))
                  : edgeNodes.map((edge) => (
                      <tr key={edge.edgeId}>
                        <td>{edge.edgeId}</td>
                        <td>{edge.runtimeId}</td>
                        <td>
                          <span className={edge.status === '健康' ? 'tag ok' : 'tag warn'}>
                            {edge.status}
                          </span>
                        </td>
                        <td>{edge.resources}</td>
                        <td>--</td>
                        <td>--</td>
                        <td>{edge.heartbeat}</td>
                      </tr>
                    ))}
              </tbody>
            </table>
          </div>
        </section>

        <section className="panel" aria-labelledby="events-title">
          <div className="panel-header">
            <h3 id="events-title">最近事件</h3>
            <span>runtime / audit</span>
          </div>
          <ul className="timeline-list">
            {(runtimeStatus?.events ?? []).slice(0, 4).map((event) => (
              <li key={`${event.edge_id}-${event.timestamp}-${event.code}`}>
                <strong>{event.message}</strong>
                <span>
                  {event.edge_id} · {event.category} · {event.severity}
                </span>
              </li>
            ))}
            {auditRecords.slice(0, 4).map((record) => (
              <li key={`${record.createdAt}-${record.action}-${record.target}`}>
                <strong>{record.action}</strong>
                <span>
                  {record.actor} · {record.target} · {record.result}
                </span>
              </li>
            ))}
            {(runtimeStatus?.events.length ?? 0) === 0 && auditRecords.length === 0 ? (
              <li>
                <strong>暂无事件</strong>
                <span>等待 runtime 或云端审计上报</span>
              </li>
            ) : null}
          </ul>
        </section>
      </div>
    </div>
  );
}

function CommandSignal({
  hint,
  label,
  tone,
  value,
}: {
  hint: string;
  label: string;
  tone?: 'warning';
  value: string;
}) {
  return (
    <article className={tone === 'warning' ? 'command-signal warning' : 'command-signal'}>
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{hint}</small>
    </article>
  );
}

function RuntimeEdgeRow({ edge }: { edge: EdgeRuntimeMetricsSnapshot }) {
  return (
    <tr>
      <td>{edge.edge_id}</td>
      <td>{edge.runtime_id}</td>
      <td>
        <span className={healthTagClass(edge.health)}>{formatHealth(edge.health)}</span>
      </td>
      <td>
        {edge.system.cpu_percent}% / {edge.system.memory_percent}% /{' '}
        {edge.system.disk_percent}%
      </td>
      <td>
        {edge.collection.active_task_count} / {edge.collection.bad_point_count}
      </td>
      <td>
        {edge.cloud_sync.desired_version} / {edge.cloud_sync.reported_version}
      </td>
      <td>{edge.cloud_sync.last_sync_seconds_ago} 秒前</td>
    </tr>
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

function healthyEdgeCount(edges: EdgeNodeResponse[]) {
  return edges.filter((edge) => edge.status === '健康').length;
}

function formatPercent(value: number | undefined) {
  if (value === undefined || Number.isNaN(value)) {
    return '--';
  }
  return `${Math.round(value)}%`;
}

function healthTagClass(health: EdgeRuntimeMetricsSnapshot['health']) {
  if (health === 'Healthy') return 'tag ok';
  if (health === 'Critical' || health === 'Offline') return 'tag danger';
  return 'tag warn';
}

function formatHealth(health: EdgeRuntimeMetricsSnapshot['health']) {
  if (health === 'Healthy') return '健康';
  if (health === 'Degraded') return '降级';
  if (health === 'Critical') return '严重';
  return '离线';
}
