import type {
  EdgeHealth,
  EdgeRuntimeMetricsSnapshot,
  ProtocolRuntimeMetrics,
  RuntimeEventSeverity,
  RuntimeStatusResponse,
} from '../api/types';

const fallbackRuntimeStatus: RuntimeStatusResponse = {
  healthyEdgeCount: 1,
  degradedEdgeCount: 0,
  criticalEdgeCount: 0,
  averageCollectionLatencyMs: 24,
  edges: [
    {
      edge_id: 'edge-dev',
      runtime_id: 'runtime-dev',
      config_version: '2026.06.26-001',
      timestamp: '2026-06-26T10:00:00Z',
      health: 'Healthy',
      system: {
        cpu_percent: 18.5,
        memory_percent: 42,
        disk_percent: 61,
        process_uptime_seconds: 3600,
      },
      collection: {
        active_task_count: 1,
        success_rate: 0.995,
        average_latency_ms: 24,
        bad_point_count: 0,
      },
      protocols: [
        {
          connection_id: 'modbus-line-a',
          protocol: 'Modbus TCP',
          connected: true,
          latency_ms: 12,
          timeout_count: 0,
          error_count: 0,
          reconnect_count: 0,
        },
      ],
      local_store: {
        backend: 'jsonl',
        buffered_records: 0,
        oldest_buffer_age_seconds: 0,
        disk_usage_percent: 35,
      },
      algorithms: [],
      cloud_sync: {
        connected: true,
        last_sync_seconds_ago: 8,
        pending_uploads: 0,
        desired_version: '2026.06.26-001',
        reported_version: '2026.06.26-001',
      },
    },
  ],
  events: [],
};

export function RuntimeStatusPage({
  runtimeStatus = fallbackRuntimeStatus,
}: {
  runtimeStatus?: RuntimeStatusResponse;
}) {
  const bufferedRecords = runtimeStatus.edges.reduce(
    (total, edge) => total + edge.local_store.buffered_records,
    0,
  );
  const pendingUploads = runtimeStatus.edges.reduce(
    (total, edge) => total + edge.cloud_sync.pending_uploads,
    0,
  );
  const protocolRows = runtimeStatus.edges.flatMap((edge) =>
    edge.protocols.map((protocol) => ({ edgeId: edge.edge_id, protocol })),
  );

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>边端运行状态</h2>
          <p>
            观察 runtime 系统资源、采集质量、协议连接、本地存储和云端同步状态，用于发布后的闭环确认。
          </p>
        </div>
      </section>

      <section className="metric-grid" aria-label="边端运行指标">
        <Metric label="健康边端" value={String(runtimeStatus.healthyEdgeCount)} hint="实时上报" />
        <Metric
          label="降级边端"
          value={String(runtimeStatus.degradedEdgeCount)}
          hint="需关注"
          tone={runtimeStatus.degradedEdgeCount > 0 ? 'alert' : undefined}
        />
        <Metric
          label="严重边端"
          value={String(runtimeStatus.criticalEdgeCount)}
          hint="需处理"
          tone={runtimeStatus.criticalEdgeCount > 0 ? 'alert' : undefined}
        />
        <Metric
          label="平均采集延迟"
          value={`${runtimeStatus.averageCollectionLatencyMs}ms`}
          hint="所有在线边端"
        />
        <Metric label="本地缓冲" value={String(bufferedRecords)} hint="待回传记录" />
        <Metric label="待上传" value={String(pendingUploads)} hint="云端同步队列" />
      </section>

      <section className="panel" aria-labelledby="runtime-edge-table-title">
        <div className="panel-header">
          <h3 id="runtime-edge-table-title">边端实时指标</h3>
          <span>最近一次 runtime metrics</span>
        </div>
        <div className="table-wrap">
          <table className="ops-table">
            <thead>
              <tr>
                <th>Edge ID</th>
                <th>健康</th>
                <th>CPU</th>
                <th>内存</th>
                <th>磁盘</th>
                <th>采集成功率</th>
                <th>平均延迟</th>
                <th>配置同步</th>
                <th>心跳</th>
              </tr>
            </thead>
            <tbody>
              {runtimeStatus.edges.map((edge) => (
                <tr key={edge.edge_id}>
                  <td>{edge.edge_id}</td>
                  <td>
                    <span className={healthTagClass(edge.health)}>
                      {formatHealth(edge.health)}
                    </span>
                  </td>
                  <td>{formatPercent(edge.system.cpu_percent)}</td>
                  <td>{formatPercent(edge.system.memory_percent)}</td>
                  <td>{formatPercent(edge.system.disk_percent)}</td>
                  <td>{formatRatio(edge.collection.success_rate)}</td>
                  <td>{edge.collection.average_latency_ms}ms</td>
                  <td>{formatSync(edge)}</td>
                  <td>{edge.cloud_sync.last_sync_seconds_ago} 秒前</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      <div className="dashboard-grid">
        <section className="panel" aria-labelledby="runtime-protocol-table-title">
          <div className="panel-header">
            <h3 id="runtime-protocol-table-title">协议连接</h3>
            <span>连接状态与错误计数</span>
          </div>
          <div className="table-wrap">
            <table className="ops-table">
              <thead>
                <tr>
                  <th>Edge ID</th>
                  <th>连接</th>
                  <th>协议</th>
                  <th>状态</th>
                  <th>延迟</th>
                  <th>超时</th>
                  <th>错误</th>
                </tr>
              </thead>
              <tbody>
                {protocolRows.map(({ edgeId, protocol }) => (
                  <ProtocolRow
                    edgeId={edgeId}
                    key={`${edgeId}:${protocol.connection_id}`}
                    protocol={protocol}
                  />
                ))}
              </tbody>
            </table>
          </div>
        </section>

        <section className="panel" aria-labelledby="runtime-events-title">
          <div className="panel-header">
            <h3 id="runtime-events-title">运行事件</h3>
            <span>协议、采集、存储、算法和同步</span>
          </div>
          {runtimeStatus.events.length > 0 ? (
            <ul className="timeline-list">
              {runtimeStatus.events.map((event) => (
                <li key={`${event.edge_id}:${event.code}:${event.timestamp}`}>
                  <strong>
                    <span className={severityTagClass(event.severity)}>
                      {formatSeverity(event.severity)}
                    </span>{' '}
                    {event.code}
                  </strong>
                  <span>
                    {event.edge_id} / {event.category} / {event.message}
                  </span>
                  <span>{formatTimestamp(event.timestamp)}</span>
                </li>
              ))}
            </ul>
          ) : (
            <ul className="detail-list">
              <li>
                <strong>暂无运行事件</strong>
                <span>边端没有上报新的异常、告警或同步事件</span>
              </li>
            </ul>
          )}
        </section>
      </div>
    </div>
  );
}

function ProtocolRow({
  edgeId,
  protocol,
}: {
  edgeId: string;
  protocol: ProtocolRuntimeMetrics;
}) {
  return (
    <tr>
      <td>{edgeId}</td>
      <td>{protocol.connection_id}</td>
      <td>{protocol.protocol}</td>
      <td>
        <span className={protocol.connected ? 'tag ok' : 'tag danger'}>
          {protocol.connected ? '已连接' : '断开'}
        </span>
      </td>
      <td>{protocol.latency_ms}ms</td>
      <td>{protocol.timeout_count}</td>
      <td>{protocol.error_count}</td>
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

function healthTagClass(health: EdgeHealth) {
  switch (health) {
    case 'Healthy':
      return 'tag ok';
    case 'Degraded':
      return 'tag warn';
    case 'Critical':
    case 'Offline':
      return 'tag danger';
  }
}

function severityTagClass(severity: RuntimeEventSeverity) {
  switch (severity) {
    case 'Info':
      return 'tag ok';
    case 'Warning':
      return 'tag warn';
    case 'Critical':
      return 'tag danger';
  }
}

function formatHealth(health: EdgeHealth) {
  switch (health) {
    case 'Healthy':
      return '健康';
    case 'Degraded':
      return '降级';
    case 'Critical':
      return '严重';
    case 'Offline':
      return '离线';
  }
}

function formatSeverity(severity: RuntimeEventSeverity) {
  switch (severity) {
    case 'Info':
      return '信息';
    case 'Warning':
      return '告警';
    case 'Critical':
      return '严重';
  }
}

function formatPercent(value: number) {
  return `${formatNumber(value)}%`;
}

function formatRatio(value: number) {
  return `${formatNumber(value * 100)}%`;
}

function formatNumber(value: number) {
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}

function formatSync(edge: EdgeRuntimeMetricsSnapshot) {
  return edge.cloud_sync.desired_version === edge.cloud_sync.reported_version
    ? '已对齐'
    : `${edge.cloud_sync.reported_version} / ${edge.cloud_sync.desired_version}`;
}

function formatTimestamp(timestamp: string) {
  return timestamp.replace('T', ' ').replace('Z', '');
}
