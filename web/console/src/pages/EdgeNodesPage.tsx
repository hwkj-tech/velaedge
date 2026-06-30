import { useState } from 'react';
import {
  Activity,
  ChevronLeft,
  ChevronRight,
  Radio,
  Save,
  Settings2,
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

export function EdgeNodesPage({
  edges = fallbackEdges,
  mqttUplink,
  onConfigureEdge,
  onSaveMqttUplink,
  onMonitorEdge,
  pageSize = 10,
}: {
  edges?: EdgeNodeResponse[];
  mqttUplink?: MqttUplinkResponse;
  onConfigureEdge?: (edgeId: string) => void;
  onSaveMqttUplink?: (edgeId: string, request: MqttUplinkResponse) => Promise<MqttUplinkResponse> | MqttUplinkResponse;
  onMonitorEdge?: (edgeId: string) => void;
  pageSize?: number;
}) {
  const [page, setPage] = useState(1);
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
                        aria-label={`配置边端 ${edge.edgeId}`}
                        className="secondary-button compact"
                        onClick={() => onConfigureEdge?.(edge.edgeId)}
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
