import { Activity, Clock3, Cpu, Database, RadioTower } from 'lucide-react';

import type {
  EdgeHealth,
  EdgeRuntimeMetricsSnapshot,
  MqttRuntimeMetrics,
  ProtocolRuntimeMetrics,
  RuntimeEventSeverity,
  RuntimeStatusResponse,
} from '../api/types';

import './RuntimeStatusPage.css';

const emptyRuntimeStatus: RuntimeStatusResponse = {
  healthyEdgeCount: 0,
  degradedEdgeCount: 0,
  criticalEdgeCount: 0,
  averageCollectionLatencyMs: 0,
  edges: [],
  events: [],
};

const emptyMqttMetrics: MqttRuntimeMetrics = {
  configured_sink_count: 0,
  connected_sink_count: 0,
  connection_generation: 0,
  publish_success_count: 0,
  publish_failure_count: 0,
  published_bytes: 0,
  sinks: [],
};

export function RuntimeStatusPage({
  focusedEdgeId,
  runtimeStatus = emptyRuntimeStatus,
}: {
  focusedEdgeId?: string;
  runtimeStatus?: RuntimeStatusResponse;
}) {
  const metricEdges = focusedEdgeId
    ? runtimeStatus.edges.filter((edge) => edge.edge_id === focusedEdgeId)
    : runtimeStatus.edges;
  const visibleEvents = focusedEdgeId
    ? runtimeStatus.events.filter((event) => event.edge_id === focusedEdgeId)
    : runtimeStatus.events;
  const protocolRows = metricEdges.flatMap((edge) =>
    edge.protocols.map((protocol) => ({ edgeId: edge.edge_id, protocol })),
  );
  const mqttRows = metricEdges.flatMap((edge) =>
    (edge.mqtt?.sinks ?? []).map((sink) => ({ edgeId: edge.edge_id, sink })),
  );
  const algorithmRows = metricEdges.flatMap((edge) =>
    edge.algorithms.map((algorithm) => ({ edgeId: edge.edge_id, algorithm })),
  );
  const bufferedRecords = sum(metricEdges, (edge) => edge.local_store.buffered_records);
  const pendingUploads = sum(metricEdges, (edge) => edge.cloud_sync.pending_uploads);
  const activeTasks = sum(metricEdges, (edge) => edge.collection.active_task_count);
  const badPoints = sum(metricEdges, (edge) => edge.collection.bad_point_count);
  const mqttConfigured = sum(
    metricEdges,
    (edge) => edge.mqtt?.configured_sink_count ?? 0,
  );
  const mqttConnected = sum(
    metricEdges,
    (edge) => edge.mqtt?.connected_sink_count ?? 0,
  );
  const mqttFailures = sum(
    metricEdges,
    (edge) => edge.mqtt?.publish_failure_count ?? 0,
  );
  const healthyEdgeCount = metricEdges.filter((edge) => edge.health === 'Healthy').length;
  const degradedEdgeCount = metricEdges.filter((edge) => edge.health === 'Degraded').length;
  const criticalEdgeCount = metricEdges.filter(
    (edge) => edge.health === 'Critical' || edge.health === 'Offline',
  ).length;
  const averageCollectionLatencyMs = average(
    metricEdges.map((edge) => edge.collection.average_latency_ms),
    focusedEdgeId ? 0 : runtimeStatus.averageCollectionLatencyMs,
  );
  const latestSnapshot = newestTimestamp(metricEdges);

  return (
    <div className="page-stack runtime-status-page">
      <section className="page-intro runtime-status-intro">
        <div>
          <h2>边端运行状态</h2>
          <p>系统资源、工业协议采集、计算节点与 MQTT 传输的实时诊断。</p>
        </div>
        <div className="runtime-live-state" role="status">
          <span className="runtime-live-dot" />
          <div>
            <strong>{focusedEdgeId ? `正在监控 ${focusedEdgeId}` : '全局实时监控'}</strong>
            <small>
              {latestSnapshot
                ? `5 秒自动刷新 · 最近上报 ${formatRelativeAge(latestSnapshot)}`
                : '等待 Runtime 上报真实指标'}
            </small>
          </div>
        </div>
      </section>

      <section className="metric-grid runtime-metric-grid" aria-label="边端运行指标">
        <Metric label="健康边端" value={String(healthyEdgeCount)} hint="实时上报" />
        <Metric
          label="降级 / 严重"
          value={`${degradedEdgeCount} / ${criticalEdgeCount}`}
          hint="需关注 / 需处理"
          tone={degradedEdgeCount + criticalEdgeCount > 0 ? 'alert' : undefined}
        />
        <Metric label="采集任务" value={String(activeTasks)} hint={`${badPoints} 个异常点位`} />
        <Metric
          label="平均采集延迟"
          value={`${averageCollectionLatencyMs}ms`}
          hint={focusedEdgeId ? '选中边端' : '在线边端平均'}
        />
        <Metric
          label="MQTT 连接"
          value={`${mqttConnected}/${mqttConfigured}`}
          hint="已连接 / 已配置"
          tone={mqttConfigured > mqttConnected ? 'alert' : undefined}
        />
        <Metric
          label="MQTT 发布失败"
          value={String(mqttFailures)}
          hint="累计确认失败"
          tone={mqttFailures > 0 ? 'alert' : undefined}
        />
        <Metric label="本地缓冲" value={String(bufferedRecords)} hint="待发送记录" />
        <Metric label="待同步" value={String(pendingUploads)} hint="Cloud 同步队列" />
      </section>

      {metricEdges.length > 0 ? (
        <section className="runtime-edge-overview" aria-label="边端实时概览">
          {metricEdges.map((edge) => (
            <EdgeOverview edge={edge} key={edge.edge_id} />
          ))}
        </section>
      ) : (
        <section className="panel runtime-empty-state">
          <Activity aria-hidden="true" size={22} />
          <div>
            <strong>{focusedEdgeId ? `尚未收到 ${focusedEdgeId} 的运行指标` : '尚未收到运行指标'}</strong>
            <span>Runtime 建立 EdgeLink 会话并完成首次指标上报后，这里会自动更新。</span>
          </div>
        </section>
      )}

      <section className="panel" aria-labelledby="runtime-mqtt-table-title">
        <div className="panel-header runtime-panel-header">
          <div>
            <span className="runtime-panel-icon"><RadioTower aria-hidden="true" size={17} /></span>
            <div>
              <h3 id="runtime-mqtt-table-title">MQTT 传输</h3>
              <span>Broker 会话、发布确认、字节与最后一次 Topic</span>
            </div>
          </div>
          <strong>{mqttConnected}/{mqttConfigured} 已连接</strong>
        </div>
        <div className="table-wrap">
          <table className="ops-table runtime-diagnostic-table">
            <thead>
              <tr>
                <th>Edge / Sink</th>
                <th>Broker</th>
                <th>会话</th>
                <th>发布确认</th>
                <th>失败</th>
                <th>数据量</th>
                <th>ACK 延迟</th>
                <th>最后发布</th>
                <th>最后 Topic / 错误</th>
              </tr>
            </thead>
            <tbody>
              {mqttRows.length > 0 ? mqttRows.map(({ edgeId, sink }) => (
                <tr key={`${edgeId}:${sink.sink_id}`}>
                  <td><strong>{edgeId}</strong><small>{sink.sink_id}</small></td>
                  <td><code>{sink.broker}</code><small>{sink.client_id}</small></td>
                  <td><span className={sink.connected ? 'tag ok' : 'tag danger'}>{sink.connected ? '已连接' : '断开'}</span></td>
                  <td>{sink.publish_success_count}</td>
                  <td className={sink.publish_failure_count > 0 ? 'runtime-danger-text' : undefined}>{sink.publish_failure_count}</td>
                  <td>{formatBytes(sink.published_bytes)}</td>
                  <td>{sink.last_ack_latency_ms == null ? `${sink.average_ack_latency_ms}ms 平均` : `${sink.last_ack_latency_ms}ms / ${sink.average_ack_latency_ms}ms 平均`}</td>
                  <td>{sink.last_publish_at ? formatTimestamp(sink.last_publish_at) : '-'}</td>
                  <td className="runtime-topic-cell">
                    <code title={sink.last_topic ?? undefined}>{sink.last_topic ?? '-'}</code>
                    {sink.last_error ? <small className="runtime-danger-text">{sink.last_error}</small> : null}
                  </td>
                </tr>
              )) : <EmptyTableRow columns={9} message="Runtime 尚未上报 MQTT Sink 指标" />}
            </tbody>
          </table>
        </div>
      </section>

      <section className="panel" aria-labelledby="runtime-protocol-table-title">
        <div className="panel-header runtime-panel-header">
          <div>
            <span className="runtime-panel-icon"><Database aria-hidden="true" size={17} /></span>
            <div>
              <h3 id="runtime-protocol-table-title">工业协议采集</h3>
              <span>连接、熔断、读写操作、质量码与订阅流</span>
            </div>
          </div>
          <strong>{protocolRows.filter(({ protocol }) => protocol.connected).length}/{protocolRows.length} 已连接</strong>
        </div>
        <div className="table-wrap">
          <table className="ops-table runtime-diagnostic-table">
            <thead>
              <tr>
                <th>Edge / 连接</th>
                <th>协议</th>
                <th>状态</th>
                <th>熔断器</th>
                <th>协议操作</th>
                <th>数据质量</th>
                <th>订阅流</th>
                <th>延迟</th>
                <th>超时 / 错误</th>
              </tr>
            </thead>
            <tbody>
              {protocolRows.length > 0 ? protocolRows.map(({ edgeId, protocol }) => (
                <ProtocolRow edgeId={edgeId} key={`${edgeId}:${protocol.connection_id}`} protocol={protocol} />
              )) : <EmptyTableRow columns={9} message="Runtime 尚未上报协议连接指标" />}
            </tbody>
          </table>
        </div>
      </section>

      <div className="dashboard-grid runtime-lower-grid">
        <section className="panel" aria-labelledby="runtime-algorithm-title">
          <div className="panel-header runtime-panel-header">
            <div>
              <span className="runtime-panel-icon"><Cpu aria-hidden="true" size={17} /></span>
              <div><h3 id="runtime-algorithm-title">计算节点</h3><span>DSL 执行延迟、错误与告警</span></div>
            </div>
          </div>
          <div className="table-wrap">
            <table className="ops-table runtime-diagnostic-table">
              <thead><tr><th>Edge / 节点</th><th>状态</th><th>最近延迟</th><th>错误</th><th>告警</th></tr></thead>
              <tbody>
                {algorithmRows.length > 0 ? algorithmRows.map(({ edgeId, algorithm }) => (
                  <tr key={`${edgeId}:${algorithm.algorithm_id}`}>
                    <td><strong>{edgeId}</strong><small>{algorithm.algorithm_id}</small></td>
                    <td><span className={algorithm.healthy ? 'tag ok' : 'tag danger'}>{algorithm.healthy ? '健康' : '异常'}</span></td>
                    <td>{algorithm.last_run_latency_ms}ms</td>
                    <td>{algorithm.error_count}</td>
                    <td>{algorithm.alert_count}</td>
                  </tr>
                )) : <EmptyTableRow columns={5} message="当前配置没有计算节点运行指标" />}
              </tbody>
            </table>
          </div>
        </section>

        <section className="panel" aria-labelledby="runtime-events-title">
          <div className="panel-header runtime-panel-header">
            <div>
              <span className="runtime-panel-icon"><Clock3 aria-hidden="true" size={17} /></span>
              <div><h3 id="runtime-events-title">运行事件</h3><span>协议、采集、存储、计算和同步</span></div>
            </div>
          </div>
          {visibleEvents.length > 0 ? (
            <ul className="timeline-list runtime-event-list">
              {visibleEvents.map((event) => (
                <li key={`${event.edge_id}:${event.code}:${event.timestamp}`}>
                  <strong><span className={severityTagClass(event.severity)}>{formatSeverity(event.severity)}</span> {event.code}</strong>
                  <span>{event.edge_id} / {event.category} / {event.message}</span>
                  <time dateTime={event.timestamp}>{formatTimestamp(event.timestamp)}</time>
                </li>
              ))}
            </ul>
          ) : <div className="runtime-table-empty">暂无新的异常、告警或同步事件</div>}
        </section>
      </div>
    </div>
  );
}

function EdgeOverview({ edge }: { edge: EdgeRuntimeMetricsSnapshot }) {
  const mqtt = edge.mqtt ?? emptyMqttMetrics;
  const publishAttempts = mqtt.publish_success_count + mqtt.publish_failure_count;
  const publishSuccessRate = publishAttempts > 0
    ? mqtt.publish_success_count / publishAttempts
    : undefined;

  return (
    <article className="runtime-edge-card">
      <header>
        <div>
          <span className={healthTagClass(edge.health)}>{formatHealth(edge.health)}</span>
          <div><strong>{edge.edge_id}</strong><small>{edge.runtime_id} · {edge.config_version}</small></div>
        </div>
        <time dateTime={edge.timestamp}>{formatRelativeAge(edge.timestamp)}</time>
      </header>
      <div className="runtime-edge-card-body">
        <div className="runtime-gauge-group" aria-label={`${edge.edge_id} 系统资源`}>
          <Gauge label="CPU" value={edge.system.cpu_percent} />
          <Gauge label="内存" value={edge.system.memory_percent} />
          <Gauge label="磁盘" value={edge.system.disk_percent} />
        </div>
        <div className="runtime-edge-facts">
          <div><span>采集成功率</span><strong>{formatRatio(edge.collection.success_rate)}</strong><small>{edge.collection.active_task_count} 任务 · {edge.collection.bad_point_count} 异常点</small></div>
          <div><span>MQTT 发布确认</span><strong>{publishSuccessRate == null ? '-' : formatRatio(publishSuccessRate)}</strong><small>{mqtt.publish_success_count} 成功 · {mqtt.publish_failure_count} 失败</small></div>
          <div><span>本地缓冲</span><strong>{edge.local_store.buffered_records}</strong><small>{edge.local_store.backend}</small></div>
          <div><span>配置同步</span><strong>{formatSync(edge)}</strong><small>{edge.cloud_sync.connected ? 'EdgeLink 已连接' : 'EdgeLink 已断开'}</small></div>
        </div>
      </div>
    </article>
  );
}

function Gauge({ label, value }: { label: string; value: number }) {
  const bounded = Math.max(0, Math.min(100, value));
  return (
    <div className="runtime-gauge">
      <span><span>{label}</span><strong>{formatPercent(value)}</strong></span>
      <div aria-label={`${label} ${formatPercent(value)}`} className="runtime-gauge-track" role="meter" aria-valuemin={0} aria-valuemax={100} aria-valuenow={bounded}>
        <i style={{ width: `${bounded}%` }} />
      </div>
    </div>
  );
}

function ProtocolRow({ edgeId, protocol }: { edgeId: string; protocol: ProtocolRuntimeMetrics }) {
  return (
    <tr>
      <td><strong>{edgeId}</strong><small>{protocol.connection_id}</small></td>
      <td>{protocol.protocol}</td>
      <td><span className={protocol.connected ? 'tag ok' : 'tag danger'}>{protocol.connected ? '已连接' : '断开'}</span></td>
      <td><span className={protocol.circuit_state === 'Open' ? 'tag danger' : 'tag'}>{formatCircuitState(protocol.circuit_state)}</span>{(protocol.consecutive_failure_count ?? 0) > 0 ? <small>连续失败 {protocol.consecutive_failure_count}</small> : null}</td>
      <td>{formatProtocolOperations(protocol)}</td>
      <td>{formatProtocolQuality(protocol)}</td>
      <td>{formatSubscriptionStream(protocol)}</td>
      <td>{protocol.latency_ms}ms</td>
      <td>{protocol.timeout_count} / {protocol.error_count}</td>
    </tr>
  );
}

function EmptyTableRow({ columns, message }: { columns: number; message: string }) {
  return <tr><td className="runtime-table-empty" colSpan={columns}>{message}</td></tr>;
}

function Metric({ hint, label, tone, value }: { hint: string; label: string; tone?: 'alert'; value: string }) {
  return <article className={tone === 'alert' ? 'metric-tile alert' : 'metric-tile'}><span>{label}</span><strong>{value}</strong><small>{hint}</small></article>;
}

function formatProtocolOperations(protocol: ProtocolRuntimeMetrics) {
  return `采集 ${protocol.collection_success_count ?? 0}/${protocol.collection_attempt_count ?? 0} · 写入 ${protocol.write_success_count ?? 0}/${protocol.write_attempt_count ?? 0}`;
}

function formatSubscriptionStream(protocol: ProtocolRuntimeMetrics) {
  const subscriptions = protocol.subscription_count ?? 0;
  const notifications = protocol.notification_count ?? 0;
  const errors = protocol.subscription_error_count ?? 0;
  const fallbacks = protocol.fallback_poll_count ?? 0;
  if (subscriptions === 0 && notifications === 0 && errors === 0 && fallbacks === 0) return '-';
  const details = [`${subscriptions} 订阅`, `${notifications} 通知`];
  if (errors > 0) details.push(`${errors} 错误`);
  if (fallbacks > 0) details.push(`${fallbacks} 次降级`);
  return details.join(' · ');
}

function formatProtocolQuality(protocol: ProtocolRuntimeMetrics) {
  const labels: Record<string, string> = {
    good: '正常', uncertain_protocol: '协议不确定', uncertain_last_known: '沿用旧值',
    uncertain_out_of_range: '超量程', uncertain_substituted: '替代值', uncertain_overflow: '溢出',
    bad_communication: '通信失败', bad_timeout: '超时', bad_protocol: '协议异常',
    bad_decode: '解码失败', bad_configuration: '配置错误', bad_out_of_service: '停止服务',
  };
  const label = protocol.last_quality_code ? labels[protocol.last_quality_code] ?? protocol.last_quality_code : '暂无采样';
  return `${label} · G ${protocol.good_value_count ?? 0} / U ${protocol.uncertain_value_count ?? 0} / B ${protocol.bad_value_count ?? 0}`;
}

function formatCircuitState(state: ProtocolRuntimeMetrics['circuit_state']) {
  if (state === 'Open') return '已熔断';
  if (state === 'HalfOpen') return '恢复探测';
  return '正常';
}

function healthTagClass(health: EdgeHealth) {
  if (health === 'Healthy') return 'tag ok';
  if (health === 'Degraded') return 'tag warn';
  return 'tag danger';
}

function severityTagClass(severity: RuntimeEventSeverity) {
  if (severity === 'Info') return 'tag ok';
  if (severity === 'Warning') return 'tag warn';
  return 'tag danger';
}

function formatHealth(health: EdgeHealth) {
  return { Healthy: '健康', Degraded: '降级', Critical: '严重', Offline: '离线' }[health];
}

function formatSeverity(severity: RuntimeEventSeverity) {
  return { Info: '信息', Warning: '告警', Critical: '严重' }[severity];
}

function formatPercent(value: number) { return `${formatNumber(value)}%`; }
function formatRatio(value: number) { return `${formatNumber(value * 100)}%`; }
function formatNumber(value: number) { return Number.isInteger(value) ? String(value) : value.toFixed(1); }

function formatSync(edge: EdgeRuntimeMetricsSnapshot) {
  return edge.cloud_sync.desired_version === edge.cloud_sync.reported_version
    ? '已对齐'
    : `${edge.cloud_sync.reported_version} / ${edge.cloud_sync.desired_version}`;
}

function formatTimestamp(timestamp: string) {
  const date = new Date(timestamp);
  return Number.isNaN(date.getTime()) ? timestamp : date.toLocaleString('zh-CN', { hour12: false });
}

function formatRelativeAge(timestamp: string) {
  const ageSeconds = Math.max(0, Math.floor((Date.now() - new Date(timestamp).getTime()) / 1000));
  if (!Number.isFinite(ageSeconds)) return timestamp;
  if (ageSeconds < 60) return `${ageSeconds} 秒前`;
  if (ageSeconds < 3600) return `${Math.floor(ageSeconds / 60)} 分钟前`;
  return formatTimestamp(timestamp);
}

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MiB`;
}

function newestTimestamp(edges: EdgeRuntimeMetricsSnapshot[]) {
  return edges.map((edge) => edge.timestamp).filter(Boolean).sort((left, right) => Date.parse(right) - Date.parse(left))[0];
}

function average(values: number[], fallback: number) {
  if (values.length === 0) return fallback;
  return Math.round(values.reduce((total, value) => total + value, 0) / values.length);
}

function sum(edges: EdgeRuntimeMetricsSnapshot[], value: (edge: EdgeRuntimeMetricsSnapshot) => number) {
  return edges.reduce((total, edge) => total + value(edge), 0);
}
