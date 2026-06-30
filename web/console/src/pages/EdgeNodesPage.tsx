import { useState } from 'react';
import {
  Activity,
  ChevronLeft,
  ChevronRight,
  Radio,
  Save,
  Settings2,
  SlidersHorizontal,
  Sparkles,
  X,
} from 'lucide-react';

import type { EdgeNodeResponse, MqttUplinkResponse } from '../api/types';

const fallbackEdges: EdgeNodeResponse[] = [
  {
    edgeId: 'edge-dev',
    displayName: '研发实验室边端',
    site: '研发/实验室',
    runtimeId: 'runtime-dev',
    status: '健康',
    resources: '18.5% / 42% / 61%',
    heartbeat: '8 秒前',
    capabilities: ['protocol:modbus-tcp', 'local-store:jsonl'],
  },
];

export type EdgeConfigTabKey =
  | 'overview'
  | 'protocol'
  | 'points'
  | 'collection'
  | 'reports'
  | 'mqtt'
  | 'release';

export interface EdgeConfigSummary {
  collectionTaskCount: number;
  dataConfigCount: number;
  edgeId: string;
  mqttSinkId: string;
  pointCount: number;
  protocolCount: number;
  releaseStatus: string;
}

export function EdgeNodesPage({
  configSummaries = [],
  edges = fallbackEdges,
  mqttUplink,
  onConfigureEdge,
  onSaveMqttUplink,
  onMonitorEdge,
  pageSize = 10,
}: {
  configSummaries?: EdgeConfigSummary[];
  edges?: EdgeNodeResponse[];
  mqttUplink?: MqttUplinkResponse;
  onConfigureEdge?: (edgeId: string, tab?: EdgeConfigTabKey) => void;
  onSaveMqttUplink?: (edgeId: string, request: MqttUplinkResponse) => Promise<MqttUplinkResponse> | MqttUplinkResponse;
  onMonitorEdge?: (edgeId: string) => void;
  pageSize?: number;
}) {
  const [page, setPage] = useState(1);
  const [configDialog, setConfigDialog] = useState<EdgeNodeResponse>();
  const [mqttDialog, setMqttDialog] = useState<{ edgeId: string; form: MqttUplinkResponse }>();
  const [saveState, setSaveState] = useState<'idle' | 'saving' | 'saved' | 'error'>('idle');
  const totalPages = Math.max(1, Math.ceil(edges.length / pageSize));
  const currentPage = Math.min(page, totalPages);
  const pageStart = (currentPage - 1) * pageSize;
  const visibleEdges = edges.slice(pageStart, pageStart + pageSize);

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>边端生命周期</h2>
          <p>
            边端由 runtime 通过 EdgeLink 主动连接后自动登记。云端负责查看运行状态、进入边端配置，并维护该边端的 MQTT 上报连接。
          </p>
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>边端实例</h3>
          <span>{edges.length} 个实例</span>
        </div>
        <div className="table-wrap">
          <table className="ops-table">
            <thead>
              <tr>
                <th>Edge ID</th>
                <th>名称</th>
                <th>站点/分组</th>
                <th>Runtime</th>
                <th>状态</th>
                <th>CPU / 内存 / 磁盘</th>
                <th>心跳</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {visibleEdges.map((edge) => (
                <tr key={edge.edgeId}>
                  <td>{edge.edgeId}</td>
                  <td>{edge.displayName}</td>
                  <td>{edge.site}</td>
                  <td>{edge.runtimeId}</td>
                  <td>
                    <span className={edge.status === '健康' ? 'tag ok' : 'tag warn'}>
                      {edge.status}
                    </span>
                  </td>
                  <td>{edge.resources}</td>
                  <td>{edge.heartbeat}</td>
                  <td>
                    <div className="row-actions">
                      <button
                        aria-label={`配置 ${edge.edgeId}`}
                        className="secondary-button compact"
                        onClick={() => setConfigDialog(edge)}
                        type="button"
                      >
                        <Settings2 size={14} aria-hidden="true" />
                        配置
                      </button>
                      <button
                        aria-label={`运行监控 ${edge.edgeId}`}
                        className="secondary-button compact"
                        onClick={() => onMonitorEdge?.(edge.edgeId)}
                        type="button"
                      >
                        <Activity size={14} aria-hidden="true" />
                        监控
                      </button>
                      <button
                        aria-label={`MQTT 配置 ${edge.edgeId}`}
                        className="secondary-button compact"
                        onClick={() => {
                          setSaveState('idle');
                          setMqttDialog({
                            edgeId: edge.edgeId,
                            form: mqttUplink ?? defaultMqttUplink(edge.edgeId),
                          });
                        }}
                        type="button"
                      >
                        <Radio size={14} aria-hidden="true" />
                        MQTT
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="pagination-bar" aria-label="边端分页">
          <span>
            第 {currentPage} / {totalPages} 页
          </span>
          <div className="row-actions">
            <button
              className="secondary-button compact"
              disabled={currentPage === 1}
              onClick={() => setPage((value) => Math.max(1, value - 1))}
              type="button"
            >
              <ChevronLeft size={14} aria-hidden="true" />
              上一页
            </button>
            <button
              className="secondary-button compact"
              disabled={currentPage === totalPages}
              onClick={() => setPage((value) => Math.min(totalPages, value + 1))}
              type="button"
            >
              <ChevronRight size={14} aria-hidden="true" />
              下一页
            </button>
          </div>
        </div>
      </section>
      {configDialog ? (
        <EdgeConfigSelectionDialog
          edge={configDialog}
          onClose={() => setConfigDialog(undefined)}
          onOpen={(tab) => {
            onConfigureEdge?.(configDialog.edgeId, tab);
            setConfigDialog(undefined);
          }}
          summary={findConfigSummary(configSummaries, configDialog.edgeId)}
        />
      ) : null}
      {mqttDialog ? (
        <div className="modal-backdrop">
          <form
            aria-label="边端 MQTT 配置"
            className="modal-panel"
            onSubmit={async (event) => {
              event.preventDefault();
              setSaveState('saving');
              try {
                const saved = await onSaveMqttUplink?.(mqttDialog.edgeId, mqttDialog.form);
                setMqttDialog(saved ? { ...mqttDialog, form: saved } : mqttDialog);
                setSaveState('saved');
              } catch {
                setSaveState('error');
              }
            }}
            role="dialog"
          >
            <div className="modal-header">
              <h3>边端 MQTT 配置 {mqttDialog.edgeId}</h3>
              <button aria-label="关闭" className="icon-button" onClick={() => setMqttDialog(undefined)} type="button">
                <X size={16} aria-hidden="true" />
              </button>
            </div>
            <div className="form-grid">
              <MqttField label="Sink ID" value={mqttDialog.form.sinkId} onChange={(sinkId) => setMqttDialog({ ...mqttDialog, form: { ...mqttDialog.form, sinkId } })} />
              <MqttField label="Broker 地址" value={mqttDialog.form.broker} onChange={(broker) => setMqttDialog({ ...mqttDialog, form: { ...mqttDialog.form, broker } })} />
              <MqttField label="Client ID" value={mqttDialog.form.clientId} onChange={(clientId) => setMqttDialog({ ...mqttDialog, form: { ...mqttDialog.form, clientId } })} />
              <MqttField label="默认 Topic 模板" value={mqttDialog.form.topicTemplate} onChange={(topicTemplate) => setMqttDialog({ ...mqttDialog, form: { ...mqttDialog.form, topicTemplate } })} />
              <MqttField label="QoS" type="number" value={String(mqttDialog.form.qos)} onChange={(qos) => setMqttDialog({ ...mqttDialog, form: { ...mqttDialog.form, qos: Number(qos) } })} />
              <MqttField label="批量条数" type="number" value={String(mqttDialog.form.batchSize)} onChange={(batchSize) => setMqttDialog({ ...mqttDialog, form: { ...mqttDialog.form, batchSize: Number(batchSize) } })} />
              <MqttField label="刷新间隔(ms)" type="number" value={String(mqttDialog.form.flushIntervalMs)} onChange={(flushIntervalMs) => setMqttDialog({ ...mqttDialog, form: { ...mqttDialog.form, flushIntervalMs: Number(flushIntervalMs) } })} />
            </div>
            <div className="drawer-footer">
              <span className="editor-status" role="status">{mqttSaveText(saveState)}</span>
              <button className="secondary-button" onClick={() => setMqttDialog(undefined)} type="button">取消</button>
              <button className="primary-button" disabled={saveState === 'saving'} type="submit">
                <Save size={15} aria-hidden="true" />
                保存
              </button>
            </div>
          </form>
        </div>
      ) : null}
    </div>
  );
}

function EdgeConfigSelectionDialog({
  edge,
  onClose,
  onOpen,
  summary,
}: {
  edge: EdgeNodeResponse;
  onClose: () => void;
  onOpen: (tab: EdgeConfigTabKey) => void;
  summary: EdgeConfigSummary;
}) {
  const recommendation = buildConfigRecommendation(summary);
  const completion = calculateConfigCompletion(summary);
  const rows: Array<{
    action: string;
    description: string;
    label: string;
    status: string;
    tab: EdgeConfigTabKey;
  }> = [
    {
      action: '配置连接',
      description: '串口总线、Modbus RTU/TCP、DL/T645 等南向采集通道',
      label: '协议连接',
      status: `${summary.protocolCount} 个连接`,
      tab: 'protocol',
    },
    {
      action: '配置点位',
      description: '协议地址到语义点位的映射',
      label: '点位配置',
      status: `${summary.pointCount} 个点位`,
      tab: 'points',
    },
    {
      action: '配置任务',
      description: '采集周期、点位批次、超时重试和缓存策略',
      label: '采集任务',
      status: `${summary.collectionTaskCount} 个任务`,
      tab: 'collection',
    },
    {
      action: '配置上报',
      description: '点位组合、DSL 算法、JSON 结构和 MQTT topic',
      label: '数据上报',
      status: `${summary.dataConfigCount} 套配置`,
      tab: 'reports',
    },
    {
      action: '配置 MQTT',
      description: 'velaMQ broker、clientId、QoS 和批量策略',
      label: 'MQTT 上报',
      status: summary.mqttSinkId,
      tab: 'mqtt',
    },
    {
      action: '配置发布',
      description: '配置差异、校验结果和 runtime 应用状态',
      label: '配置发布',
      status: summary.releaseStatus,
      tab: 'release',
    },
  ];

  return (
    <div className="modal-backdrop">
      <section
        aria-label="配置边端"
        className="modal-panel edge-config-select-modal"
        role="dialog"
      >
        <div className="modal-header">
          <div>
            <h3>配置边端</h3>
            <p>{edge.displayName} · {edge.edgeId} · {edge.runtimeId}</p>
          </div>
          <button aria-label="关闭" className="icon-button" onClick={onClose} type="button">
            <X size={16} aria-hidden="true" />
          </button>
        </div>

        <div className="edge-config-select-summary">
          <span className={edge.status === '健康' ? 'tag ok' : 'tag warn'}>{edge.status}</span>
          <strong>{edge.resources}</strong>
          <small>{edge.heartbeat}</small>
        </div>

        <div className="edge-config-readiness">
          <div>
            <span>配置完整度</span>
            <strong>{completion}%</strong>
          </div>
          <div className="readiness-track" aria-label="配置完整度">
            <span style={{ width: `${completion}%` }} />
          </div>
          <small>{readinessText(completion)}</small>
        </div>

        <div className="edge-agent-recommendation">
          <Sparkles size={16} aria-hidden="true" />
          <div>
            <strong>{recommendation.title}</strong>
            <p>{recommendation.detail}</p>
          </div>
          <button
            className="secondary-button compact"
            onClick={() => onOpen(recommendation.tab)}
            type="button"
          >
            {recommendation.action}
          </button>
        </div>

        <div className="binding-matrix">
          {rows.map((row) => (
            <div className="binding-row" key={row.label}>
              <div>
                <strong>{row.label}</strong>
                <p>{row.description}</p>
              </div>
              <span className="binding-status">{row.status}</span>
              <button
                className="secondary-button compact"
                onClick={() => onOpen(row.tab)}
                type="button"
              >
                {row.action}
              </button>
            </div>
          ))}
        </div>

        <div className="drawer-footer">
          <button className="secondary-button" onClick={onClose} type="button">取消</button>
          <button className="primary-button" onClick={() => onOpen('overview')} type="button">
            <SlidersHorizontal size={15} aria-hidden="true" />
            打开配置总览
          </button>
        </div>
      </section>
    </div>
  );
}

function calculateConfigCompletion(summary: EdgeConfigSummary) {
  const score = [
    summary.protocolCount > 0,
    summary.pointCount > 0,
    summary.collectionTaskCount > 0,
    summary.dataConfigCount > 0,
    summary.mqttSinkId !== '未配置',
    !summary.releaseStatus.includes('待'),
  ].filter(Boolean).length;
  return Math.round((score / 6) * 100);
}

function readinessText(completion: number) {
  if (completion >= 84) return '采集、处理、上报和发布链路基本闭环';
  if (completion >= 50) return '核心链路已具备，仍需补齐发布或上报配置';
  return '建议先补齐采集连接、点位和任务配置';
}

function buildConfigRecommendation(summary: EdgeConfigSummary): {
  action: string;
  detail: string;
  tab: EdgeConfigTabKey;
  title: string;
} {
  if (summary.protocolCount === 0) {
    return {
      action: '去配置连接',
      detail: '该边端还没有南向采集连接，建议先维护串口或 Modbus 连接。',
      tab: 'protocol',
      title: '建议先补齐采集连接',
    };
  }
  if (summary.pointCount === 0) {
    return {
      action: '去配置点位',
      detail: '已有连接但还没有语义点位，建议先导入或维护点位映射。',
      tab: 'points',
      title: '建议生成点位映射',
    };
  }
  if (summary.dataConfigCount === 0) {
    return {
      action: '去配置上报',
      detail: '点位和采集链路已具备，下一步可以组合点位、算法和 MQTT topic。',
      tab: 'reports',
      title: '建议创建数据上报配置',
    };
  }
  if (summary.releaseStatus.includes('待') || summary.releaseStatus.includes('下发')) {
    return {
      action: '去发布',
      detail: '配置已具备，建议校验差异并发布到 runtime。',
      tab: 'release',
      title: '建议校验并发布配置',
    };
  }
  return {
    action: '看总览',
    detail: '采集、处理和上报链路已形成闭环，可从总览继续检查绑定关系。',
    tab: 'overview',
    title: '配置链路状态良好',
  };
}

function findConfigSummary(
  summaries: EdgeConfigSummary[],
  edgeId: string,
): EdgeConfigSummary {
  return (
    summaries.find((summary) => summary.edgeId === edgeId) ?? {
      collectionTaskCount: 0,
      dataConfigCount: 0,
      edgeId,
      mqttSinkId: '未配置',
      pointCount: 0,
      protocolCount: 0,
      releaseStatus: '待发布',
    }
  );
}

function MqttField({
  label,
  onChange,
  type = 'text',
  value,
}: {
  label: string;
  onChange: (value: string) => void;
  type?: string;
  value: string;
}) {
  return (
    <label className="editor-control">
      <span>{label}</span>
      <input aria-label={label} type={type} value={value} onChange={(event) => onChange(event.target.value)} />
    </label>
  );
}

function defaultMqttUplink(edgeId: string): MqttUplinkResponse {
  return {
    batchSize: 100,
    broker: 'mqtts://velamq.local:8883',
    clientId: `${edgeId}-runtime`,
    flushIntervalMs: 1000,
    qos: 1,
    sinkId: 'velamq-main',
    topicTemplate: 'edge/{edge_id}/device/{device_id}/telemetry',
  };
}

function mqttSaveText(state: 'idle' | 'saving' | 'saved' | 'error') {
  if (state === 'saving') return '正在保存';
  if (state === 'saved') return '已保存';
  if (state === 'error') return '保存失败';
  return '等待保存';
}
