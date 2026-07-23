import { useState } from 'react';
import {
  Activity,
  ChevronLeft,
  ChevronRight,
  KeyRound,
  Plus,
  Radio,
  Save,
  Trash2,
  X,
} from 'lucide-react';

import type {
  EdgeNodeResponse,
  ManagementActionResponse,
  MqttUplinkResponse,
} from '../api/types';
import { Modal } from '../components/Modal';
import { displayError } from '../utils/errors';

export type EdgeConfigTabKey =
  | 'versions'
  | 'protocol'
  | 'points'
  | 'collection'
  | 'algorithms'
  | 'reports'
  | 'mqtt'
  | 'release';

export interface EdgeConfigSummary {
  collectionTaskCount: number;
  dataConfigCount: number;
  edgeId: string;
  mqttSinkId: string;
  pointCount: number;
  productName?: string;
  productVersion?: string;
  projectName?: string;
  protocolCount: number;
  releaseStatus: string;
}

export interface EdgeProductOption {
  productId: string;
  productName: string;
  projectId: string;
  projectName: string;
  version: string;
}

export interface CreateManagedEdgeRequest {
  displayName: string;
  productId: string;
  projectId: string;
  site: string;
}

export function EdgeNodesPage({
  accessTokens = {},
  configSummaries = [],
  edges = [],
  mqttUplink,
  onCreateEdge,
  onDeleteEdge,
  onGenerateAccessToken,
  onSaveMqttUplink,
  pageSize = 10,
  products = [],
}: {
  accessTokens?: Record<string, string>;
  configSummaries?: EdgeConfigSummary[];
  edges?: EdgeNodeResponse[];
  mqttUplink?: MqttUplinkResponse;
  onCreateEdge?: (
    request: CreateManagedEdgeRequest,
  ) => Promise<EdgeNodeResponse> | EdgeNodeResponse;
  onDeleteEdge?: (edgeId: string) => Promise<void> | void;
  onGenerateAccessToken?: (edgeId: string) => Promise<string> | string;
  onSaveMqttUplink?: (edgeId: string, request: MqttUplinkResponse) => Promise<MqttUplinkResponse> | MqttUplinkResponse;
  pageSize?: number;
  products?: EdgeProductOption[];
}) {
  const [page, setPage] = useState(1);
  const [createDialog, setCreateDialog] = useState<CreateManagedEdgeRequest>();
  const [accessDialog, setAccessDialog] = useState<EdgeNodeResponse>();
  const [monitorDialog, setMonitorDialog] = useState<EdgeNodeResponse>();
  const [mqttDialog, setMqttDialog] = useState<{ edgeId: string; form: MqttUplinkResponse }>();
  const [issuedAccessTokens, setIssuedAccessTokens] = useState<Record<string, string>>({});
  const [saveState, setSaveState] = useState<'idle' | 'saving' | 'saved' | 'error'>('idle');
  const [toolbarMessage, setToolbarMessage] = useState('');
  const totalPages = Math.max(1, Math.ceil(edges.length / pageSize));
  const currentPage = Math.min(page, totalPages);
  const pageStart = (currentPage - 1) * pageSize;
  const visibleEdges = edges.slice(pageStart, pageStart + pageSize);

  const handleDeleteEdge = async (edgeId: string) => {
    setToolbarMessage('');
    try {
      await onDeleteEdge?.(edgeId);
      setToolbarMessage(`已移除边端 ${edgeId}`);
    } catch (error) {
      setToolbarMessage(`移除边端失败：${displayError(error, '请确认 runtime 已离线或未上报')}`);
    }
  };
  const defaultProduct = products[0];
  const openCreateDialog = () => {
    setSaveState('idle');
    setCreateDialog({
      displayName: '新边端',
      productId: defaultProduct?.productId ?? '',
      projectId: defaultProduct?.projectId ?? '',
      site: '待分配',
    });
  };
  const updateCreateDialog = (patch: Partial<CreateManagedEdgeRequest>) => {
    setCreateDialog((current) => (current ? { ...current, ...patch } : current));
  };
  const visibleAccessToken = accessDialog
    ? issuedAccessTokens[accessDialog.edgeId] ?? accessTokens[accessDialog.edgeId]
    : undefined;

  const handleGenerateAccessToken = async () => {
    if (!accessDialog || !onGenerateAccessToken) return;
    setSaveState('saving');
    try {
      const token = await onGenerateAccessToken(accessDialog.edgeId);
      setIssuedAccessTokens((current) => ({
        ...current,
        [accessDialog.edgeId]: token,
      }));
      setSaveState('saved');
    } catch (error) {
      setSaveState('error');
      setToolbarMessage(`生成接入 token 失败：${displayError(error)}`);
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>边端管理</h2>
          <p>手动登记边端，绑定产品，生成 runtime 接入 token。</p>
        </div>
        <div className="toolbar">
          {toolbarMessage ? (
            <span className="toolbar-status" role="status">
              {toolbarMessage}
            </span>
          ) : null}
          <button className="primary-button" onClick={openCreateDialog} type="button">
            <Plus size={15} aria-hidden="true" />
            新增边端
          </button>
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
                <th>项目</th>
                <th>关联产品</th>
                <th>Runtime</th>
                <th>状态</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {visibleEdges.length === 0 ? (
                <tr>
                  <td className="table-empty-cell" colSpan={7}>
                    暂无边端实例，请新增边端并使用接入 Token 启动 Runtime
                  </td>
                </tr>
              ) : null}
              {visibleEdges.map((edge) => {
                const summary = findConfigSummary(configSummaries, edge.edgeId);

                return (
                  <tr key={edge.edgeId}>
                    <td>{edge.edgeId}</td>
                    <td>{edge.displayName}</td>
                    <td>{summary.projectName ?? '未分配项目'}</td>
                    <td>
                      <div className="edge-product-cell">
                        <strong>{summary.productName ?? '未绑定产品'}</strong>
                        <span>{summary.productVersion ?? '-'}</span>
                      </div>
                    </td>
                    <td>{edge.runtimeId}</td>
                    <td>
                      <span className={edge.status === '健康' ? 'tag ok' : 'tag warn'}>
                        {edge.status}
                      </span>
                    </td>
                    <td>
                      <div className="row-actions">
                        <button
                          aria-label={`接入信息 ${edge.edgeId}`}
                          className="secondary-button compact"
                          onClick={() => setAccessDialog(edge)}
                          type="button"
                        >
                          <KeyRound size={14} aria-hidden="true" />
                          接入
                        </button>
                        <button
                          aria-label={`运行监控 ${edge.edgeId}`}
                          className="secondary-button compact"
                          onClick={() => setMonitorDialog(edge)}
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
                        {canRemoveEdge(edge) ? (
                          <button
                            aria-label={`移除边端 ${edge.edgeId}`}
                            className="danger-button compact"
                            onClick={() => {
                              void handleDeleteEdge(edge.edgeId);
                            }}
                            type="button"
                          >
                            <Trash2 size={14} aria-hidden="true" />
                            移除
                          </button>
                        ) : null}
                      </div>
                    </td>
                  </tr>
                );
              })}
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
      {createDialog ? (
        <Modal onClose={() => setCreateDialog(undefined)}>
          <form
            aria-label="新增边端"
            className="modal-panel"
            onSubmit={async (event) => {
              event.preventDefault();
              setSaveState('saving');
              try {
                const created = await onCreateEdge?.(createDialog);
                if (created?.accessToken) {
                  setIssuedAccessTokens((current) => ({
                    ...current,
                    [created.edgeId]: created.accessToken as string,
                  }));
                }
                setToolbarMessage(
                  created
                    ? `已创建边端 ${created.edgeId}，token 已生成`
                    : '已创建边端，token 已生成',
                );
                setSaveState('saved');
                setCreateDialog(undefined);
                if (created) setAccessDialog(created);
              } catch (error) {
                setSaveState('error');
                setToolbarMessage(`创建边端失败：${displayError(error)}`);
              }
            }}
            role="dialog"
          >
            <div className="modal-header">
              <div>
                <h3>新增边端</h3>
                <p>选择产品后生成接入 token，runtime 使用 token 主动连接 cloud。</p>
              </div>
              <button
                aria-label="关闭"
                className="icon-button"
                onClick={() => setCreateDialog(undefined)}
                type="button"
              >
                <X size={16} aria-hidden="true" />
              </button>
            </div>
            <div className="form-grid">
              <EdgeTextField
                label="边端名称"
                onChange={(displayName) => updateCreateDialog({ displayName })}
                value={createDialog.displayName}
              />
              <EdgeTextField
                label="站点/分组"
                onChange={(site) => updateCreateDialog({ site })}
                value={createDialog.site}
              />
              <label className="editor-control form-wide">
                <span>关联产品</span>
                <select
                  aria-label="关联产品"
                  onChange={(event) => {
                    const product = products.find(
                      (item) => item.productId === event.target.value,
                    );
                    updateCreateDialog({
                      productId: event.target.value,
                      projectId: product?.projectId ?? createDialog.projectId,
                    });
                  }}
                  value={createDialog.productId}
                >
                  {products.map((product) => (
                    <option key={product.productId} value={product.productId}>
                      {product.projectName} / {product.productName} · {product.version}
                    </option>
                  ))}
                </select>
              </label>
            </div>
            <div className="drawer-footer">
              <span className="editor-status" role="status">{edgeCreateText(saveState)}</span>
              <button className="secondary-button" onClick={() => setCreateDialog(undefined)} type="button">
                取消
              </button>
              <button className="primary-button" disabled={!createDialog.productId || saveState === 'saving'} type="submit">
                生成接入 token
              </button>
            </div>
          </form>
        </Modal>
      ) : null}
      {accessDialog ? (
        <Modal onClose={() => setAccessDialog(undefined)}>
          <section aria-label="边端接入信息" className="modal-panel" role="dialog">
            <div className="modal-header">
              <div>
                <h3>边端接入信息</h3>
                <p>{accessDialog.displayName} · {accessDialog.edgeId}</p>
              </div>
              <button aria-label="关闭" className="icon-button" onClick={() => setAccessDialog(undefined)} type="button">
                <X size={16} aria-hidden="true" />
              </button>
            </div>
            <div className="edge-access-card">
              <span>Edge ID</span>
              <strong>{accessDialog.edgeId}</strong>
              <span>接入 Token</span>
              {visibleAccessToken ? (
                <code>{visibleAccessToken}</code>
              ) : (
                <p className="edge-access-once-note">
                  Token 仅在创建或重新生成时显示，Cloud 不保存明文。
                </p>
              )}
              {visibleAccessToken ? (
                <>
                  <span>接入命令</span>
                  <code>
                    edge-runtime --cloud-gateway-addr cloud:7443 --edge-id {accessDialog.edgeId} --access-token {visibleAccessToken}
                  </code>
                </>
              ) : null}
            </div>
            <div className="drawer-footer">
              <span className="editor-status" role="status">
                {saveState === 'saving'
                  ? '正在生成'
                  : saveState === 'saved'
                    ? '新 token 已生成，旧 token 已失效'
                    : saveState === 'error'
                      ? '生成失败'
                      : '请妥善保存一次性 token'}
              </span>
              <button
                className="secondary-button"
                disabled={!onGenerateAccessToken || saveState === 'saving'}
                onClick={() => void handleGenerateAccessToken()}
                type="button"
              >
                <KeyRound size={15} aria-hidden="true" />
                重新生成 token
              </button>
            </div>
          </section>
        </Modal>
      ) : null}
      {monitorDialog ? (
        <Modal onClose={() => setMonitorDialog(undefined)}>
          <section aria-label="边端运行监控" className="modal-panel edge-monitor-modal" role="dialog">
            <div className="modal-header">
              <div>
                <h3>边端运行监控</h3>
                <p>{monitorDialog.displayName} · {monitorDialog.edgeId}</p>
              </div>
              <button aria-label="关闭" className="icon-button" onClick={() => setMonitorDialog(undefined)} type="button">
                <X size={16} aria-hidden="true" />
              </button>
            </div>
            <EdgeMonitorDetails
              edge={monitorDialog}
              summary={findConfigSummary(configSummaries, monitorDialog.edgeId)}
            />
          </section>
        </Modal>
      ) : null}
      {mqttDialog ? (
        <Modal onClose={() => setMqttDialog(undefined)}>
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
              } catch (error) {
                setSaveState('error');
                setToolbarMessage(`保存 MQTT 配置失败：${displayError(error)}`);
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
              <MqttField label="MQTT 用户名" value={mqttDialog.form.username ?? ''} onChange={(username) => setMqttDialog({ ...mqttDialog, form: { ...mqttDialog.form, username } })} />
              <MqttField label="密码环境变量" value={mqttDialog.form.passwordEnv ?? ''} onChange={(passwordEnv) => setMqttDialog({ ...mqttDialog, form: { ...mqttDialog.form, passwordEnv } })} />
              <MqttField label="私有 CA 路径" value={mqttDialog.form.tlsCaPath ?? ''} onChange={(tlsCaPath) => setMqttDialog({ ...mqttDialog, form: { ...mqttDialog.form, tlsCaPath } })} />
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
        </Modal>
      ) : null}
    </div>
  );
}

function canRemoveEdge(edge: EdgeNodeResponse) {
  return edge.status === '未上报' || edge.status === '离线';
}

function EdgeMonitorDetails({ edge, summary }: { edge: EdgeNodeResponse; summary: EdgeConfigSummary }) {
  const completion = calculateConfigCompletion(summary);
  const resourceParts = edge.resources.split('/').map((value) => value.trim());

  return (
    <div className="edge-monitor-content">
      <div className="edge-monitor-status">
        <div>
          <span>运行状态</span>
          <strong className={edge.status === '健康' ? 'monitor-health healthy' : 'monitor-health warning'}>
            <i />{edge.status}
          </strong>
        </div>
        <div><span>Runtime</span><strong>{edge.runtimeId}</strong></div>
        <div><span>最近心跳</span><strong>{edge.heartbeat}</strong></div>
        <div><span>MQTT Sink</span><strong>{summary.mqttSinkId}</strong></div>
      </div>

      <section className="edge-monitor-section">
        <div className="edge-monitor-section-title">
          <div><span>CONFIGURATION</span><h4>配置完整度</h4></div>
          <strong>{completion}%</strong>
        </div>
        <div className="edge-monitor-progress"><span style={{ width: `${completion}%` }} /></div>
        <div className="edge-monitor-config-grid">
          <MonitorMetric label="协议连接" value={summary.protocolCount} suffix="个" />
          <MonitorMetric label="采集点位" value={summary.pointCount} suffix="个" />
          <MonitorMetric label="采集任务" value={summary.collectionTaskCount} suffix="个" />
          <MonitorMetric label="数据上报" value={summary.dataConfigCount} suffix="个" />
        </div>
        <div className="edge-monitor-release">
          <span>配置发布</span><strong>{summary.releaseStatus}</strong>
        </div>
      </section>

      <section className="edge-monitor-section">
        <div className="edge-monitor-section-title"><div><span>RESOURCES</span><h4>资源使用</h4></div></div>
        <div className="edge-monitor-resource-grid">
          <ResourceMetric label="CPU" value={resourceParts[0] ?? '-'} />
          <ResourceMetric label="内存" value={resourceParts[1] ?? '-'} />
          <ResourceMetric label="磁盘" value={resourceParts[2] ?? '-'} />
        </div>
      </section>
    </div>
  );
}

function MonitorMetric({ label, suffix, value }: { label: string; suffix: string; value: number }) {
  return <article className="edge-monitor-metric"><span>{label}</span><strong>{value}<small>{suffix}</small></strong></article>;
}

function ResourceMetric({ label, value }: { label: string; value: string }) {
  const numericValue = Number.parseFloat(value);
  const width = Number.isFinite(numericValue) ? Math.min(100, Math.max(0, numericValue)) : 0;
  return (
    <article className="edge-resource-metric">
      <div><span>{label}</span><strong>{value}</strong></div>
      <div className="edge-resource-track"><span style={{ width: `${width}%` }} /></div>
    </article>
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
      productName: '未绑定产品',
      productVersion: '-',
      projectName: '未分配项目',
    }
  );
}

function EdgeTextField({
  label,
  onChange,
  value,
}: {
  label: string;
  onChange: (value: string) => void;
  value: string;
}) {
  return (
    <label className="editor-control">
      <span>{label}</span>
      <input
        aria-label={label}
        onChange={(event) => onChange(event.target.value)}
        value={value}
      />
    </label>
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
    broker: '',
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

function edgeCreateText(state: 'idle' | 'saving' | 'saved' | 'error') {
  if (state === 'saving') return '创建中';
  if (state === 'saved') return '已创建';
  if (state === 'error') return '创建失败';
  return '等待创建';
}
