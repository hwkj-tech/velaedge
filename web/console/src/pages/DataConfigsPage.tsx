import { useMemo, useState } from 'react';
import { Edit3, Plus, Trash2, X } from 'lucide-react';

import type {
  DataConfigPoint,
  DataConfigResponse,
  EdgeNodeResponse,
  MqttUplinkResponse,
  ProtocolConnectionResponse,
  SaveDataConfigRequest,
} from '../api/types';
import { DataTable, type DataTableColumn } from '../components/DataTable';
import './PointMappingsPage.css';

const emptyPoint: DataConfigPoint = {
  addressKind: 'holding_register',
  addressValue: '40001',
  jsonField: 'pressure',
  pointId: 'pressure',
  semanticId: 'pump.pressure',
  unit: 'MPa',
  valueType: 'float32',
};

const fallbackConfig: DataConfigResponse = {
  edgeId: 'edge-dev',
  configId: 'pump_status',
  name: '泵状态上报',
  enabled: true,
  deviceId: 'pump-1',
  protocolConnectionId: 'modbus-line-a',
  collection: { periodMs: 1000, retryCount: 2, timeoutMs: 800 },
  points: [emptyPoint],
  publish: {
    payload: { includeQuality: true, mode: 'object', timestampField: 'ts' },
    qos: 1,
    sinkId: 'velamq-main',
    topicTemplate: 'factory/{edge_id}/{device_id}/status',
  },
};

export function DataConfigsPage({
  configs = [fallbackConfig],
  edges = [],
  mqttUplink,
  onDeleteConfig,
  onSaveConfig,
  onSelectEdge,
  protocolConnections = [],
  selectedEdgeId = configs[0]?.edgeId ?? 'edge-dev',
}: {
  configs?: DataConfigResponse[];
  edges?: EdgeNodeResponse[];
  mqttUplink?: MqttUplinkResponse | null;
  onDeleteConfig?: (edgeId: string, configId: string) => Promise<void> | void;
  onSaveConfig?: (
    edgeId: string,
    configId: string | null,
    request: SaveDataConfigRequest,
  ) => Promise<void> | void;
  onSelectEdge?: (edgeId: string) => Promise<void> | void;
  protocolConnections?: ProtocolConnectionResponse[];
  selectedEdgeId?: string;
}) {
  const [dialog, setDialog] = useState<
    { mode: 'create'; form: SaveDataConfigRequest } | { mode: 'edit'; configId: string; form: SaveDataConfigRequest }
  >();
  const [step, setStep] = useState(0);
  const [status, setStatus] = useState('');
  const activeConfigs = configs.filter((config) => config.edgeId === selectedEdgeId);
  const edge = edges.find((item) => item.edgeId === selectedEdgeId);
  const columns = useMemo<Array<DataTableColumn<DataConfigResponse>>>(
    () => [
      {
        key: 'config',
        header: '配置',
        render: (config) => (
          <button
            className="point-id-button"
            onClick={() => openEdit(config)}
            type="button"
          >
            {config.configId}
          </button>
        ),
        width: '18%',
      },
      { key: 'name', header: '名称', render: (config) => config.name },
      {
        key: 'collection',
        header: '采集',
        render: (config) => `${config.collection.periodMs}ms / ${config.points.length} 点`,
      },
      {
        key: 'mqtt',
        header: 'MQTT',
        render: (config) => `${config.publish.sinkId} / QoS ${config.publish.qos}`,
      },
      {
        key: 'topic',
        header: 'Topic',
        render: (config) => config.publish.topicTemplate,
      },
      {
        key: 'status',
        header: '状态',
        render: (config) => (
          <span className={config.enabled ? 'status-pill success' : 'status-pill'}>
            {config.enabled ? '启用' : '暂停'}
          </span>
        ),
        width: '90px',
      },
      {
        key: 'actions',
        header: '操作',
        render: (config) => (
          <div className="row-actions">
            <button className="secondary-button compact" onClick={() => openEdit(config)} type="button">
              <Edit3 size={14} aria-hidden="true" />
              编辑
            </button>
            <button
              className="secondary-button compact icon-only"
              aria-label={`删除 ${config.configId}`}
              onClick={() => {
                void handleDelete(config.configId);
              }}
              type="button"
            >
              <Trash2 size={14} aria-hidden="true" />
            </button>
          </div>
        ),
        width: '170px',
      },
    ],
    [selectedEdgeId],
  );

  const openCreate = () => {
    setStep(0);
    setStatus('');
    setDialog({ mode: 'create', form: createDefaultForm(selectedEdgeId, mqttUplink, protocolConnections) });
  };

  function openEdit(config: DataConfigResponse) {
    setStep(0);
    setStatus('');
    setDialog({ configId: config.configId, form: responseToSave(config), mode: 'edit' });
  }

  const updateForm = (patch: Partial<SaveDataConfigRequest>) => {
    setDialog((current) =>
      current ? { ...current, form: { ...current.form, ...patch } } : current,
    );
  };

  const updateCollection = (patch: Partial<SaveDataConfigRequest['collection']>) => {
    setDialog((current) =>
      current
        ? { ...current, form: { ...current.form, collection: { ...current.form.collection, ...patch } } }
        : current,
    );
  };

  const updatePublish = (patch: Partial<SaveDataConfigRequest['publish']>) => {
    setDialog((current) =>
      current
        ? { ...current, form: { ...current.form, publish: { ...current.form.publish, ...patch } } }
        : current,
    );
  };

  const updatePayload = (patch: Partial<SaveDataConfigRequest['publish']['payload']>) => {
    setDialog((current) =>
      current
        ? {
            ...current,
            form: {
              ...current.form,
              publish: {
                ...current.form.publish,
                payload: { ...current.form.publish.payload, ...patch },
              },
            },
          }
        : current,
    );
  };

  const updateFirstPoint = (patch: Partial<DataConfigPoint>) => {
    setDialog((current) => {
      if (!current) return current;
      const first = current.form.points[0] ?? emptyPoint;
      return {
        ...current,
        form: { ...current.form, points: [{ ...first, ...patch }, ...current.form.points.slice(1)] },
      };
    });
  };

  const handleSave = async () => {
    if (!dialog) return;
    setStatus('保存中');
    try {
      await onSaveConfig?.(
        selectedEdgeId,
        dialog.mode === 'edit' ? dialog.configId : null,
        sanitizeForm(dialog.form),
      );
      setStatus('已保存');
      setDialog(undefined);
    } catch {
      setStatus('保存失败');
    }
  };

  const handleDelete = async (configId: string) => {
    setStatus('');
    try {
      await onDeleteConfig?.(selectedEdgeId, configId);
      setStatus(`已删除 ${configId}`);
    } catch {
      setStatus('删除失败');
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>数据配置</h2>
          <p>定义一次采集、周期、点位映射和 MQTT JSON 上报。一个边端可配置多套数据流。</p>
        </div>
        <div className="toolbar">
          <label className="release-edge-select">
            <span>配置边端</span>
            <select
              aria-label="配置边端"
              value={selectedEdgeId}
              onChange={(event) => {
                void onSelectEdge?.(event.target.value);
              }}
            >
              {(edges.length ? edges : [{ edgeId: selectedEdgeId, displayName: edge?.displayName ?? selectedEdgeId } as EdgeNodeResponse]).map((item) => (
                <option key={item.edgeId} value={item.edgeId}>
                  {item.displayName} / {item.edgeId}
                </option>
              ))}
            </select>
          </label>
          {status ? <span className="toolbar-status" role="status">{status}</span> : null}
          <button className="primary-button" onClick={openCreate} type="button">
            <Plus size={15} aria-hidden="true" />
            新建数据配置
          </button>
        </div>
      </section>

      <section className="table-card">
        <div className="section-heading">
          <h3>数据配置清单</h3>
          <span>{activeConfigs.length} 套配置</span>
        </div>
        <DataTable
          ariaLabel="数据配置分页"
          columns={columns}
          getRowKey={(config) => config.configId}
          pageSize={10}
          rows={activeConfigs}
        />
      </section>

      {dialog ? (
        <div className="modal-backdrop">
          <form
            aria-label={dialog.mode === 'create' ? '新建数据配置' : '编辑数据配置'}
            className="modal-panel data-config-modal"
            onSubmit={(event) => {
              event.preventDefault();
              if (step < steps.length - 1) {
                setStep((value) => value + 1);
              } else {
                void handleSave();
              }
            }}
            role="dialog"
          >
            <div className="modal-header">
              <h3>{dialog.mode === 'create' ? '新建数据配置' : `编辑数据配置 ${dialog.form.configId}`}</h3>
              <button aria-label="关闭" className="icon-button" onClick={() => setDialog(undefined)} type="button">
                <X size={16} aria-hidden="true" />
              </button>
            </div>
            <div className="step-tabs" aria-label="配置步骤">
              {steps.map((label, index) => (
                <button
                  className={index === step ? 'step-tab active' : 'step-tab'}
                  key={label}
                  onClick={() => setStep(index)}
                  type="button"
                >
                  {index + 1}. {label}
                </button>
              ))}
            </div>
            <DataConfigStep
              form={dialog.form}
              mqttUplink={mqttUplink}
              protocolConnections={protocolConnections}
              step={step}
              updateCollection={updateCollection}
              updateFirstPoint={updateFirstPoint}
              updateForm={updateForm}
              updatePayload={updatePayload}
              updatePublish={updatePublish}
            />
            <div className="drawer-footer">
              <span className="editor-status">{status}</span>
              <button className="secondary-button" onClick={() => setDialog(undefined)} type="button">取消</button>
              {step > 0 ? (
                <button className="secondary-button" onClick={() => setStep((value) => value - 1)} type="button">
                  上一步
                </button>
              ) : null}
              <button className="primary-button" type="submit">
                {step < steps.length - 1 ? '下一步' : '保存'}
              </button>
            </div>
          </form>
        </div>
      ) : null}
    </div>
  );
}

const steps = ['基础信息', '采集设置', '点位映射', 'MQTT 上报', 'JSON 预览'];

function DataConfigStep({
  form,
  mqttUplink,
  protocolConnections,
  step,
  updateCollection,
  updateFirstPoint,
  updateForm,
  updatePayload,
  updatePublish,
}: {
  form: SaveDataConfigRequest;
  mqttUplink?: MqttUplinkResponse | null;
  protocolConnections: ProtocolConnectionResponse[];
  step: number;
  updateCollection: (patch: Partial<SaveDataConfigRequest['collection']>) => void;
  updateFirstPoint: (patch: Partial<DataConfigPoint>) => void;
  updateForm: (patch: Partial<SaveDataConfigRequest>) => void;
  updatePayload: (patch: Partial<SaveDataConfigRequest['publish']['payload']>) => void;
  updatePublish: (patch: Partial<SaveDataConfigRequest['publish']>) => void;
}) {
  const firstPoint = form.points[0] ?? emptyPoint;
  if (step === 0) {
    return (
      <div className="form-grid">
        <TextField label="配置 ID" value={form.configId} onChange={(configId) => updateForm({ configId })} />
        <TextField label="配置名称" value={form.name} onChange={(name) => updateForm({ name })} />
        <TextField label="设备 ID" value={form.deviceId} onChange={(deviceId) => updateForm({ deviceId })} />
        <label className="editor-control">
          <span>启用</span>
          <select aria-label="启用" value={form.enabled ? 'true' : 'false'} onChange={(event) => updateForm({ enabled: event.target.value === 'true' })}>
            <option value="true">启用</option>
            <option value="false">暂停</option>
          </select>
        </label>
      </div>
    );
  }
  if (step === 1) {
    return (
      <div className="form-grid">
        <label className="editor-control">
          <span>协议连接</span>
          <select aria-label="协议连接" value={form.protocolConnectionId} onChange={(event) => updateForm({ protocolConnectionId: event.target.value })}>
            {protocolConnections.length ? protocolConnections.map((connection) => (
              <option key={connection.connectionId} value={connection.connectionId}>
                {connection.connectionId} / {connection.protocol}
              </option>
            )) : <option value={form.protocolConnectionId}>{form.protocolConnectionId}</option>}
          </select>
        </label>
        <TextField label="采集周期(ms)" type="number" value={String(form.collection.periodMs)} onChange={(periodMs) => updateCollection({ periodMs: numberOr(periodMs, 1000) })} />
        <TextField label="超时(ms)" type="number" value={String(form.collection.timeoutMs)} onChange={(timeoutMs) => updateCollection({ timeoutMs: numberOr(timeoutMs, 800) })} />
        <TextField label="重试次数" type="number" value={String(form.collection.retryCount)} onChange={(retryCount) => updateCollection({ retryCount: numberOr(retryCount, 2) })} />
      </div>
    );
  }
  if (step === 2) {
    return (
      <div className="form-grid">
        <TextField label="Point ID" value={firstPoint.pointId} onChange={(pointId) => updateFirstPoint({ pointId })} />
        <TextField label="语义 ID" value={firstPoint.semanticId} onChange={(semanticId) => updateFirstPoint({ semanticId })} />
        <TextField label="地址类型" value={firstPoint.addressKind} onChange={(addressKind) => updateFirstPoint({ addressKind })} />
        <TextField label="地址值" value={firstPoint.addressValue} onChange={(addressValue) => updateFirstPoint({ addressValue })} />
        <TextField label="数据类型" value={firstPoint.valueType} onChange={(valueType) => updateFirstPoint({ valueType })} />
        <TextField label="JSON 字段" value={firstPoint.jsonField} onChange={(jsonField) => updateFirstPoint({ jsonField })} />
      </div>
    );
  }
  if (step === 3) {
    return (
      <div className="form-grid">
        <TextField label="MQTT Sink" value={form.publish.sinkId || mqttUplink?.sinkId || ''} onChange={(sinkId) => updatePublish({ sinkId })} />
        <TextField label="MQTT Topic" value={form.publish.topicTemplate} onChange={(topicTemplate) => updatePublish({ topicTemplate })} />
        <TextField label="QoS" type="number" value={String(form.publish.qos)} onChange={(qos) => updatePublish({ qos: numberOr(qos, 1) })} />
        <label className="editor-control">
          <span>Payload 模式</span>
          <select aria-label="Payload 模式" value={form.publish.payload.mode} onChange={(event) => updatePayload({ mode: event.target.value as 'object' | 'array' })}>
            <option value="object">object</option>
            <option value="array">array</option>
          </select>
        </label>
        <TextField label="时间字段" value={form.publish.payload.timestampField} onChange={(timestampField) => updatePayload({ timestampField })} />
      </div>
    );
  }
  return (
    <label className="editor-control preview-field">
      <span>JSON 预览</span>
      <textarea aria-label="JSON 预览" readOnly value={JSON.stringify(buildPreview(form), null, 2)} />
    </label>
  );
}

function TextField({ label, onChange, type = 'text', value }: { label: string; onChange: (value: string) => void; type?: string; value: string }) {
  return (
    <label className="editor-control">
      <span>{label}</span>
      <input aria-label={label} type={type} value={value} onChange={(event) => onChange(event.target.value)} />
    </label>
  );
}

function createDefaultForm(
  edgeId: string,
  mqttUplink?: MqttUplinkResponse | null,
  protocolConnections: ProtocolConnectionResponse[] = [],
): SaveDataConfigRequest {
  return {
    collection: { periodMs: 1000, retryCount: 2, timeoutMs: 800 },
    configId: `${edgeId}_data_${Date.now().toString().slice(-4)}`,
    deviceId: 'pump-1',
    enabled: true,
    name: '新数据配置',
    points: [emptyPoint],
    protocolConnectionId: protocolConnections[0]?.connectionId ?? 'modbus-line-a',
    publish: {
      payload: { includeQuality: true, mode: 'object', timestampField: 'ts' },
      qos: mqttUplink?.qos ?? 1,
      sinkId: mqttUplink?.sinkId ?? 'velamq-main',
      topicTemplate: 'factory/{edge_id}/{device_id}/telemetry',
    },
  };
}

function responseToSave(config: DataConfigResponse): SaveDataConfigRequest {
  const { edgeId: _edgeId, ...request } = config;
  return request;
}

function sanitizeForm(form: SaveDataConfigRequest): SaveDataConfigRequest {
  return {
    ...form,
    collection: {
      periodMs: Math.max(Number(form.collection.periodMs) || 1000, 100),
      retryCount: Math.max(Number(form.collection.retryCount) || 0, 0),
      timeoutMs: Math.max(Number(form.collection.timeoutMs) || 800, 1),
    },
    points: form.points.map((point) => ({
      ...point,
      unit: point.unit || '-',
    })),
    publish: {
      ...form.publish,
      qos: Math.min(Math.max(Number(form.publish.qos) || 0, 0), 2),
    },
  };
}

function buildPreview(form: SaveDataConfigRequest) {
  const values = Object.fromEntries(form.points.map((point) => [point.jsonField, sampleValue(point.valueType)]));
  const quality = Object.fromEntries(form.points.map((point) => [point.jsonField, 'good']));
  return {
    config_id: form.configId,
    device_id: form.deviceId,
    [form.publish.payload.timestampField]: '2026-06-30T00:00:00Z',
    quality: form.publish.payload.includeQuality ? quality : undefined,
    values,
  };
}

function sampleValue(valueType: string) {
  if (valueType.includes('bool')) return true;
  if (valueType.includes('int')) return 1;
  if (valueType.includes('string')) return 'sample';
  return 1.23;
}

function numberOr(value: string, fallback: number) {
  return Number.isFinite(Number(value)) ? Number(value) : fallback;
}
