import { type DragEvent, useMemo, useState } from 'react';
import { Copy, Edit3, GitBranch, Pause, Play, Plus, Radio, Trash2, X } from 'lucide-react';

import type {
  AlgorithmResponse,
  DataConfigPoint,
  DataConfigGraphNodeKind,
  DataConfigResponse,
  EdgeNodeResponse,
  MqttUplinkResponse,
  PointMappingResponse,
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
  algorithmIds: ['pump-anomaly-v1'],
  visualGraph: {
    edges: [
      { edgeId: 'pressure-to-algorithm', from: 'point-pressure', to: 'algorithm-pump-anomaly-v1' },
      { edgeId: 'algorithm-to-json', from: 'algorithm-pump-anomaly-v1', to: 'json-payload' },
      { edgeId: 'json-to-mqtt', from: 'json-payload', to: 'mqtt-output' },
    ],
    nodes: [
      { kind: 'point', label: 'pressure', nodeId: 'point-pressure', refId: 'pressure', x: 80, y: 88 },
      { kind: 'algorithm', label: 'pump-anomaly-v1', nodeId: 'algorithm-pump-anomaly-v1', refId: 'pump-anomaly-v1', x: 280, y: 88 },
      { kind: 'json', label: 'JSON Payload', nodeId: 'json-payload', refId: null, x: 500, y: 88 },
      { kind: 'mqtt', label: 'MQTT Topic', nodeId: 'mqtt-output', refId: 'factory/{edge_id}/{device_id}/status', x: 700, y: 88 },
    ],
  },
  publish: {
    payload: { includeQuality: true, mode: 'object', timestampField: 'ts' },
    qos: 1,
    sinkId: 'velamq-main',
    topicTemplate: 'factory/{edge_id}/{device_id}/status',
  },
};

export function DataConfigsPage({
  algorithms = [],
  configs = [fallbackConfig],
  edges = [],
  embedded = false,
  mqttUplink,
  onDeleteConfig,
  onSaveConfig,
  pointMappings = [],
  protocolConnections = [],
  selectedEdgeId = configs[0]?.edgeId ?? 'edge-dev',
}: {
  algorithms?: AlgorithmResponse[];
  configs?: DataConfigResponse[];
  edges?: EdgeNodeResponse[];
  embedded?: boolean;
  mqttUplink?: MqttUplinkResponse | null;
  onDeleteConfig?: (edgeId: string, configId: string) => Promise<void> | void;
  onSaveConfig?: (
    edgeId: string,
    configId: string | null,
    request: SaveDataConfigRequest,
  ) => Promise<void> | void;
  pointMappings?: PointMappingResponse[];
  protocolConnections?: ProtocolConnectionResponse[];
  selectedEdgeId?: string;
}) {
  const [dialog, setDialog] = useState<
    { mode: 'create'; form: SaveDataConfigRequest } | { mode: 'edit'; configId: string; form: SaveDataConfigRequest }
  >();
  const [step, setStep] = useState(0);
  const [status, setStatus] = useState('');
  const [query, setQuery] = useState('');
  const [enabledFilter, setEnabledFilter] = useState<'all' | 'enabled' | 'disabled'>('all');
  const [validationErrors, setValidationErrors] = useState<string[]>([]);
  const activeConfigs = configs.filter((config) => config.edgeId === selectedEdgeId);
  const filteredConfigs = activeConfigs.filter((config) => {
    const keyword = query.trim().toLowerCase();
    const matchesKeyword = keyword
      ? [
          config.configId,
          config.name,
          config.deviceId,
          config.protocolConnectionId,
          config.publish.sinkId,
          config.publish.topicTemplate,
          ...config.points.map((point) => point.pointId),
          ...config.points.map((point) => point.semanticId),
          ...(config.algorithmIds ?? []),
        ]
          .join(' ')
          .toLowerCase()
          .includes(keyword)
      : true;
    const matchesState =
      enabledFilter === 'all' ||
      (enabledFilter === 'enabled' && config.enabled) ||
      (enabledFilter === 'disabled' && !config.enabled);
    return matchesKeyword && matchesState;
  });
  const activeAlgorithms = algorithms.filter((algorithm) => algorithm.edgeId === selectedEdgeId);
  const activePointMappings = pointMappings.filter((point) => point.edgeId === selectedEdgeId);
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
        header: '输入点位',
        render: (config) => `${config.points.length} 点`,
      },
      {
        key: 'algorithms',
        header: '算法',
        render: (config) =>
          config.algorithmIds?.length ? config.algorithmIds.join(', ') : '未启用',
      },
      {
        key: 'mqtt',
        header: 'MQTT',
        render: (config) => `QoS ${config.publish.qos}`,
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
              className="secondary-button compact"
              onClick={() => {
                void handleDuplicate(config);
              }}
              type="button"
            >
              <Copy size={14} aria-hidden="true" />
              复制
            </button>
            <button
              className="secondary-button compact"
              onClick={() => {
                void handleToggleEnabled(config);
              }}
              type="button"
            >
              {config.enabled ? <Pause size={14} aria-hidden="true" /> : <Play size={14} aria-hidden="true" />}
              {config.enabled ? '暂停' : '启用'}
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
        width: '270px',
      },
    ],
    [selectedEdgeId],
  );

  const openCreate = () => {
    setStep(0);
    setStatus('');
    setValidationErrors([]);
    setDialog({
      mode: 'create',
      form: createDefaultForm(selectedEdgeId, mqttUplink, protocolConnections, activePointMappings),
    });
  };

  function openEdit(config: DataConfigResponse) {
    setStep(0);
    setStatus('');
    setValidationErrors([]);
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
    const nextErrors = validateDataConfigForm(dialog.form);
    setValidationErrors(nextErrors);
    if (nextErrors.length > 0) {
      setStatus('请先修正配置');
      return;
    }
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

  const handleDuplicate = async (config: DataConfigResponse) => {
    setStatus('');
    try {
      const request = sanitizeForm({
        ...responseToSave(config),
        configId: nextDuplicateConfigId(config.configId, activeConfigs.map((item) => item.configId)),
        name: `${config.name} 副本`,
      });
      await onSaveConfig?.(selectedEdgeId, null, request);
      setStatus(`已复制 ${config.configId}`);
    } catch {
      setStatus('复制失败');
    }
  };

  const handleToggleEnabled = async (config: DataConfigResponse) => {
    setStatus('');
    try {
      await onSaveConfig?.(
        selectedEdgeId,
        config.configId,
        sanitizeForm({ ...responseToSave(config), enabled: !config.enabled }),
      );
      setStatus(`${config.enabled ? '已暂停' : '已启用'} ${config.configId}`);
    } catch {
      setStatus('状态更新失败');
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>数据配置</h2>
          <p>定义数据上报流水线：选择边端已配置点位，拖入算法节点汇聚处理，再组装 JSON 上报到 MQTT topic。</p>
        </div>
        <div className="toolbar">
          {status ? <span className="toolbar-status" role="status">{status}</span> : null}
          <button className="primary-button" onClick={openCreate} type="button">
            <Plus size={15} aria-hidden="true" />
            新建数据上报
          </button>
        </div>
      </section>

      <section className="table-card">
        <div className="section-heading">
          <h3>数据上报清单</h3>
          <span>
            {filteredConfigs.length === activeConfigs.length
              ? `${activeConfigs.length} 套配置`
              : `已筛选 ${filteredConfigs.length} / ${activeConfigs.length} 套配置`}
          </span>
        </div>
        <div className="table-filter-bar" aria-label="数据上报筛选">
          <label>
            <span>搜索</span>
            <input
              aria-label="搜索数据上报"
              placeholder="配置 ID、名称、Topic、点位"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </label>
          <label>
            <span>状态</span>
            <select
              aria-label="状态筛选"
              value={enabledFilter}
              onChange={(event) => setEnabledFilter(event.target.value as 'all' | 'enabled' | 'disabled')}
            >
              <option value="all">全部状态</option>
              <option value="enabled">仅启用</option>
              <option value="disabled">仅暂停</option>
            </select>
          </label>
        </div>
        <DataTable
          ariaLabel="数据配置分页"
          columns={columns}
          getRowKey={(config) => config.configId}
          pageSize={10}
          rows={filteredConfigs}
        />
      </section>

      {dialog ? (
        <div className="modal-backdrop">
          <form
            aria-label={dialog.mode === 'create' ? '新建数据上报' : '编辑数据上报'}
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
              <h3>{dialog.mode === 'create' ? '新建数据上报' : `编辑数据上报 ${dialog.form.configId}`}</h3>
              <button aria-label="关闭" className="icon-button" onClick={() => setDialog(undefined)} type="button">
                <X size={16} aria-hidden="true" />
              </button>
            </div>
            <div className="step-tabs" aria-label="配置步骤">
              {steps.map((label, index) => (
                <button
                  className={index === step ? 'step-tab active' : 'step-tab'}
                  key={label}
                  onClick={() => {
                    setStep(index);
                    setValidationErrors([]);
                  }}
                  type="button"
                >
                  {index + 1}. {label}
                </button>
              ))}
            </div>
            <DataConfigStep
              form={dialog.form}
              mqttUplink={mqttUplink}
              pointMappings={activePointMappings}
              protocolConnections={protocolConnections}
              step={step}
              updateCollection={updateCollection}
              updateFirstPoint={updateFirstPoint}
              updateForm={updateForm}
              updateAlgorithmIds={(algorithmIds) => updateForm({ algorithmIds })}
              updatePayload={updatePayload}
              updatePublish={updatePublish}
              algorithms={activeAlgorithms}
            />
            {validationErrors.length ? (
              <div className="form-validation-panel" role="alert">
                <strong>保存前需要处理</strong>
                <ul>
                  {validationErrors.map((message) => (
                    <li key={message}>{message}</li>
                  ))}
                </ul>
              </div>
            ) : null}
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

const steps = ['基础信息', '可视化编排', '上报规则', 'JSON 预览'];

function DataConfigStep({
  algorithms,
  form,
  mqttUplink,
  pointMappings,
  protocolConnections,
  step,
  updateCollection,
  updateFirstPoint,
  updateForm,
  updateAlgorithmIds,
  updatePayload,
  updatePublish,
}: {
  algorithms: AlgorithmResponse[];
  form: SaveDataConfigRequest;
  mqttUplink?: MqttUplinkResponse | null;
  pointMappings: PointMappingResponse[];
  protocolConnections: ProtocolConnectionResponse[];
  step: number;
  updateCollection: (patch: Partial<SaveDataConfigRequest['collection']>) => void;
  updateFirstPoint: (patch: Partial<DataConfigPoint>) => void;
  updateForm: (patch: Partial<SaveDataConfigRequest>) => void;
  updateAlgorithmIds: (algorithmIds: string[]) => void;
  updatePayload: (patch: Partial<SaveDataConfigRequest['publish']['payload']>) => void;
  updatePublish: (patch: Partial<SaveDataConfigRequest['publish']>) => void;
}) {
  const firstPoint = form.points[0] ?? emptyPoint;
  const graph = form.visualGraph ?? createDefaultVisualGraph(form);
  if (step === 0) {
    return (
      <div className="form-grid">
        <DataConfigReadiness
          algorithmCount={algorithms.length}
          mqttUplink={mqttUplink}
          pointCount={pointMappings.length}
        />
        <TextField label="配置 ID" value={form.configId} onChange={(configId) => updateForm({ configId })} />
        <TextField label="配置名称" value={form.name} onChange={(name) => updateForm({ name })} />
        <TextField label="设备 ID" value={form.deviceId} onChange={(deviceId) => updateForm({ deviceId })} />
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
        <label className="editor-control">
          <span>启用</span>
          <select aria-label="启用" value={form.enabled ? 'true' : 'false'} onChange={(event) => updateForm({ enabled: event.target.value === 'true' })}>
            <option value="true">启用</option>
            <option value="false">暂停</option>
          </select>
        </label>
        <div className="info-panel">
          采集周期和点位采集任务在“采集任务”页面单独配置。本页面只定义已采集点位如何汇聚、组装 JSON 并上报。
        </div>
      </div>
    );
  }
  if (step === 1) {
    return (
      <VisualReportBuilder
        algorithms={algorithms}
        form={form}
        graph={graph}
        pointMappings={pointMappings}
        updateAlgorithmIds={updateAlgorithmIds}
        updateFirstPoint={updateFirstPoint}
        updateForm={updateForm}
      />
    );
  }
  if (step === 2) {
    return (
      <div className="form-grid">
        <div className="info-panel">
          MQTT Broker、Client ID、认证与 TLS 在“边端管理”的边端级 MQTT 配置中维护。这里仅定义本条数据流的 topic 和 payload。
        </div>
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

function DataConfigReadiness({
  algorithmCount,
  mqttUplink,
  pointCount,
}: {
  algorithmCount: number;
  mqttUplink?: MqttUplinkResponse | null;
  pointCount: number;
}) {
  return (
    <div className="data-config-readiness-grid">
      <ReadinessItem label="可选点位" ready={pointCount > 0} value={`${pointCount} 个`} />
      <ReadinessItem label="可选算法" ready={algorithmCount > 0} value={algorithmCount > 0 ? `${algorithmCount} 个` : '可选'} />
      <ReadinessItem label="MQTT Sink" ready={Boolean(mqttUplink?.sinkId)} value={mqttUplink?.sinkId ?? '未配置'} />
    </div>
  );
}

function ReadinessItem({
  label,
  ready,
  value,
}: {
  label: string;
  ready: boolean;
  value: string;
}) {
  return (
    <div className={ready ? 'readiness-item ready' : 'readiness-item'}>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
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

function VisualReportBuilder({
  algorithms,
  form,
  graph,
  pointMappings,
  updateAlgorithmIds,
  updateFirstPoint,
  updateForm,
}: {
  algorithms: AlgorithmResponse[];
  form: SaveDataConfigRequest;
  graph: NonNullable<SaveDataConfigRequest['visualGraph']>;
  pointMappings: PointMappingResponse[];
  updateAlgorithmIds: (algorithmIds: string[]) => void;
  updateFirstPoint: (patch: Partial<DataConfigPoint>) => void;
  updateForm: (patch: Partial<SaveDataConfigRequest>) => void;
}) {
  const [pointQuery, setPointQuery] = useState('');
  const [algorithmQuery, setAlgorithmQuery] = useState('');
  const pointResources = pointMappings.length
    ? pointMappings.map((point) => ({
        addressKind: point.address.split(':')[0] || 'holding_register',
        addressValue: point.address.split(':').slice(1).join(':') || point.pointId,
        jsonField: point.pointId,
        pointId: point.pointId,
        semanticId: point.semanticTelemetry,
        unit: point.unit,
        valueType: point.valueType,
      }))
    : form.points;
  const selectedPointIds = new Set(form.points.map((point) => point.pointId));
  const selectedAlgorithmIds = new Set(form.algorithmIds ?? []);
  const filteredPointResources = pointResources.filter((point) =>
    matchesResourceQuery(
      [point.pointId, point.semanticId, point.addressKind, point.addressValue, point.valueType],
      pointQuery,
    ),
  );
  const filteredAlgorithms = algorithms.filter((algorithm) =>
    matchesResourceQuery(
      [
        algorithm.algorithmId,
        algorithm.algorithmKind,
        ...(algorithm.inputIds ?? []),
        ...(algorithm.outputIds ?? []),
      ],
      algorithmQuery,
    ),
  );

  const addPoint = (point: DataConfigPoint) => {
    const points = selectedPointIds.has(point.pointId) ? form.points : [...form.points, point];
    updateForm({
      points,
      visualGraph: addGraphNode(graph, 'point', point.pointId, point.pointId, form.publish.topicTemplate),
    });
  };

  const addAlgorithm = (algorithm: AlgorithmResponse) => {
    const algorithmIds = selectedAlgorithmIds.has(algorithm.algorithmId)
      ? form.algorithmIds ?? []
      : [...(form.algorithmIds ?? []), algorithm.algorithmId];
    updateAlgorithmIds(algorithmIds);
    updateForm({
      algorithmIds,
      visualGraph: addGraphNode(graph, 'algorithm', algorithm.algorithmId, algorithm.algorithmId, form.publish.topicTemplate),
    });
  };

  const removePoint = (pointId: string) => {
    const points = form.points.filter((point) => point.pointId !== pointId);
    updateForm({
      points,
      visualGraph: removeGraphNode(graph, `point-${pointId}`, form.publish.topicTemplate),
    });
  };

  const removeAlgorithm = (algorithmId: string) => {
    const algorithmIds = (form.algorithmIds ?? []).filter((id) => id !== algorithmId);
    updateAlgorithmIds(algorithmIds);
    updateForm({
      algorithmIds,
      visualGraph: removeGraphNode(graph, `algorithm-${algorithmId}`, form.publish.topicTemplate),
    });
  };

  const ensureOutput = () => {
    updateForm({ visualGraph: ensureOutputNodes(graph, form.publish.topicTemplate) });
  };

  const addFilteredPoints = () => {
    const merged = mergePoints(form.points, filteredPointResources);
    updateForm({
      points: merged,
      visualGraph: ensureOutputNodes(
        {
          edges: graph.edges,
          nodes: [
            ...graph.nodes.filter((node) => node.kind !== 'point' && node.kind !== 'json' && node.kind !== 'mqtt'),
            ...merged.map((point, index) => ({
              kind: 'point' as const,
              label: point.pointId,
              nodeId: `point-${point.pointId}`,
              refId: point.pointId,
              x: 56,
              y: 56 + index * 86,
            })),
          ],
        },
        form.publish.topicTemplate,
      ),
    });
  };

  const clearPoints = () => {
    updateForm({
      points: [],
      visualGraph: ensureOutputNodes(
        {
          edges: graph.edges.filter((edge) => !edge.from.startsWith('point-') && !edge.to.startsWith('point-')),
          nodes: graph.nodes.filter((node) => node.kind !== 'point'),
        },
        form.publish.topicTemplate,
      ),
    });
  };

  const onDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    const [kind, refId] = event.dataTransfer.getData('application/x-edge-node').split(':');
    if (kind === 'point') {
      const point = pointResources.find((item) => item.pointId === refId);
      if (point) addPoint(point);
    }
    if (kind === 'algorithm') {
      const algorithm = algorithms.find((item) => item.algorithmId === refId);
      if (algorithm) addAlgorithm(algorithm);
    }
  };

  return (
    <div className="visual-builder">
      <aside className="node-palette" aria-label="节点库">
        <div>
          <div className="palette-heading">
            <h4>已配置点位</h4>
            <span>{filteredPointResources.length} / {pointResources.length}</span>
          </div>
          <label className="palette-search">
            <span>搜索点位资源</span>
            <input
              aria-label="搜索点位资源"
              placeholder="点位、语义、地址"
              value={pointQuery}
              onChange={(event) => setPointQuery(event.target.value)}
            />
          </label>
          <div className="palette-actions">
            <button className="secondary-button compact" onClick={addFilteredPoints} type="button">
              加入筛选点位
            </button>
            <button className="secondary-button compact" onClick={clearPoints} type="button">
              清空点位
            </button>
          </div>
          {filteredPointResources.map((point) => (
            <button
              className="palette-node"
              draggable
              key={point.pointId}
              onClick={() => addPoint(point)}
              onDragStart={(event) => event.dataTransfer.setData('application/x-edge-node', `point:${point.pointId}`)}
              type="button"
            >
              <GitBranch size={14} aria-hidden="true" />
              <span>{point.pointId}</span>
              <small>{point.semanticId}</small>
            </button>
          ))}
          {filteredPointResources.length === 0 ? <p className="palette-empty">没有匹配的点位。</p> : null}
        </div>
        <div>
          <div className="palette-heading">
            <h4>算法节点</h4>
            <span>{filteredAlgorithms.length} / {algorithms.length}</span>
          </div>
          <label className="palette-search">
            <span>搜索算法资源</span>
            <input
              aria-label="搜索算法资源"
              placeholder="算法 ID、类型、输入输出"
              value={algorithmQuery}
              onChange={(event) => setAlgorithmQuery(event.target.value)}
            />
          </label>
          {filteredAlgorithms.length ? filteredAlgorithms.map((algorithm) => (
            <button
              className="palette-node"
              draggable
              key={algorithm.algorithmId}
              onClick={() => addAlgorithm(algorithm)}
              onDragStart={(event) => event.dataTransfer.setData('application/x-edge-node', `algorithm:${algorithm.algorithmId}`)}
              type="button"
            >
              <Radio size={14} aria-hidden="true" />
              <span>{algorithm.algorithmId}</span>
              <small>{algorithm.algorithmKind}</small>
            </button>
          )) : <p className="palette-empty">暂无匹配算法，可直接上报原始点位。</p>}
        </div>
      </aside>
      <section
        className="report-canvas"
        onDragOver={(event) => event.preventDefault()}
        onDrop={onDrop}
      >
        <div className="canvas-toolbar">
          <strong>数据上报画布</strong>
          <button className="secondary-button compact" onClick={ensureOutput} type="button">
            生成 JSON/MQTT 节点
          </button>
        </div>
        <div className="canvas-lane" aria-label="数据上报画布">
          {ensureOutputNodes(graph, form.publish.topicTemplate).nodes.map((node) => (
            <div
              className={`canvas-node ${node.kind}`}
              key={node.nodeId}
              style={{ left: node.x, top: node.y }}
            >
              <span>{nodeKindText(node.kind)}</span>
              <strong>{node.label}</strong>
              {node.refId ? <small>{node.refId}</small> : null}
            </div>
          ))}
          <div className="canvas-flow-line" />
        </div>
        <div className="selected-flow-summary">
          <span>{form.points.length} 个点位</span>
          <span>{(form.algorithmIds ?? []).length} 个算法</span>
          <span>输出到 {form.publish.topicTemplate}</span>
        </div>
        <div className="selected-flow-assets" aria-label="已选数据流资源">
          <section>
            <h4>已选点位</h4>
            <div className="asset-chip-list">
              {form.points.map((point) => (
                <span className="asset-chip point" key={point.pointId}>
                  <strong>{point.jsonField}</strong>
                  <small>{point.pointId}</small>
                  <button
                    aria-label={`移除点位 ${point.pointId}`}
                    onClick={() => removePoint(point.pointId)}
                    type="button"
                  >
                    <X size={12} aria-hidden="true" />
                  </button>
                </span>
              ))}
            </div>
          </section>
          <section>
            <h4>已选算法</h4>
            <div className="asset-chip-list">
              {(form.algorithmIds ?? []).length ? (form.algorithmIds ?? []).map((algorithmId) => (
                <span className="asset-chip algorithm" key={algorithmId}>
                  <strong>{algorithmId}</strong>
                  <small>DSL</small>
                  <button
                    aria-label={`移除算法 ${algorithmId}`}
                    onClick={() => removeAlgorithm(algorithmId)}
                    type="button"
                  >
                    <X size={12} aria-hidden="true" />
                  </button>
                </span>
              )) : <p>未选择算法，点位将直接组装为 JSON 上报。</p>}
            </div>
          </section>
        </div>
      </section>
    </div>
  );
}

function createDefaultForm(
  edgeId: string,
  mqttUplink?: MqttUplinkResponse | null,
  protocolConnections: ProtocolConnectionResponse[] = [],
  pointMappings: PointMappingResponse[] = [],
): SaveDataConfigRequest {
  const initialPoints = pointMappings.length ? [] : [emptyPoint];
  const firstPoint = pointMappings[0];
  const firstConnectionId =
    firstPoint?.connection ||
    protocolConnections[0]?.connectionId ||
    'modbus-line-a';

  return {
    collection: { periodMs: 1000, retryCount: 2, timeoutMs: 800 },
    configId: `${edgeId}_data_${Date.now().toString().slice(-4)}`,
    deviceId: firstPoint?.deviceId || 'pump-1',
    enabled: true,
    algorithmIds: [],
    visualGraph: createDefaultVisualGraph({
      points: initialPoints,
      publish: {
        payload: { includeQuality: true, mode: 'object', timestampField: 'ts' },
        qos: mqttUplink?.qos ?? 1,
        sinkId: mqttUplink?.sinkId ?? 'velamq-main',
        topicTemplate: 'factory/{edge_id}/{device_id}/telemetry',
      },
    }),
    name: '新数据配置',
    points: initialPoints,
    protocolConnectionId: firstConnectionId,
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
  return {
    ...request,
    algorithmIds: request.algorithmIds ?? [],
    visualGraph: request.visualGraph ?? createDefaultVisualGraph(request),
  };
}

function sanitizeForm(form: SaveDataConfigRequest): SaveDataConfigRequest {
  return {
    ...form,
    algorithmIds: form.algorithmIds ?? [],
    visualGraph: ensureOutputNodes(form.visualGraph ?? createDefaultVisualGraph(form), form.publish.topicTemplate),
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

function validateDataConfigForm(form: SaveDataConfigRequest) {
  const errors: string[] = [];
  if (!form.configId.trim()) {
    errors.push('配置 ID 不能为空');
  }
  if (!form.name.trim()) {
    errors.push('配置名称不能为空');
  }
  if (!form.deviceId.trim()) {
    errors.push('设备 ID 不能为空');
  }
  if (!form.protocolConnectionId.trim()) {
    errors.push('协议连接不能为空');
  }
  if (form.points.length === 0 || form.points.every((point) => !point.pointId.trim())) {
    errors.push('至少选择 1 个上报点位');
  }
  const duplicatedJsonField = findDuplicate(
    form.points.map((point) => point.jsonField.trim()).filter(Boolean),
  );
  if (duplicatedJsonField) {
    errors.push(`JSON 字段 ${duplicatedJsonField} 重复`);
  }
  if (!form.publish.sinkId.trim()) {
    errors.push('MQTT Sink 不能为空');
  }
  if (!form.publish.topicTemplate.trim()) {
    errors.push('MQTT Topic 不能为空');
  }
  return errors;
}

function matchesResourceQuery(values: string[], query: string) {
  const keyword = query.trim().toLowerCase();
  if (!keyword) return true;
  return values.join(' ').toLowerCase().includes(keyword);
}

function mergePoints(currentPoints: DataConfigPoint[], nextPoints: DataConfigPoint[]) {
  const existing = new Set(currentPoints.map((point) => point.pointId));
  return [
    ...currentPoints,
    ...nextPoints.filter((point) => !existing.has(point.pointId)),
  ];
}

function findDuplicate(values: string[]) {
  const seen = new Set<string>();
  for (const value of values) {
    if (seen.has(value)) {
      return value;
    }
    seen.add(value);
  }
  return null;
}

function nextDuplicateConfigId(configId: string, existingIds: string[]) {
  const existing = new Set(existingIds);
  let index = 1;
  let candidate = `${configId}_copy`;
  while (existing.has(candidate)) {
    index += 1;
    candidate = `${configId}_copy_${index}`;
  }
  return candidate;
}

function buildPreview(form: SaveDataConfigRequest) {
  const values = Object.fromEntries(form.points.map((point) => [point.jsonField, sampleValue(point.valueType)]));
  const quality = Object.fromEntries(form.points.map((point) => [point.jsonField, 'good']));
  return {
    config_id: form.configId,
    device_id: form.deviceId,
    algorithms: form.algorithmIds ?? [],
    graph: {
      nodes: (form.visualGraph?.nodes ?? []).map((node) => ({ id: node.nodeId, kind: node.kind, ref: node.refId })),
    },
    [form.publish.payload.timestampField]: '2026-06-30T00:00:00Z',
    quality: form.publish.payload.includeQuality ? quality : undefined,
    values,
  };
}

function createDefaultVisualGraph(form: Pick<SaveDataConfigRequest, 'points' | 'publish'>) {
  const nodes = form.points.map((point, index) => ({
    kind: 'point' as const,
    label: point.pointId,
    nodeId: `point-${point.pointId}`,
    refId: point.pointId,
    x: 56,
    y: 56 + index * 86,
  }));
  return ensureOutputNodes({ edges: [], nodes }, form.publish.topicTemplate);
}

function addGraphNode(
  graph: NonNullable<SaveDataConfigRequest['visualGraph']>,
  kind: DataConfigGraphNodeKind,
  label: string,
  refId: string,
  topicTemplate: string,
) {
  const nodeId = `${kind}-${refId}`;
  if (graph.nodes.some((node) => node.nodeId === nodeId)) {
    return ensureOutputNodes(graph, topicTemplate);
  }
  const next = {
    edges: graph.edges,
    nodes: [
      ...graph.nodes.filter((node) => node.kind !== 'json' && node.kind !== 'mqtt'),
      {
        kind,
        label,
        nodeId,
        refId,
        x: kind === 'point' ? 56 : 286,
        y: 56 + graph.nodes.filter((node) => node.kind === kind).length * 86,
      },
    ],
  };
  return ensureOutputNodes(next, topicTemplate);
}

function removeGraphNode(
  graph: NonNullable<SaveDataConfigRequest['visualGraph']>,
  nodeId: string,
  topicTemplate: string,
) {
  return ensureOutputNodes(
    {
      edges: graph.edges.filter((edge) => edge.from !== nodeId && edge.to !== nodeId),
      nodes: graph.nodes.filter((node) => node.nodeId !== nodeId),
    },
    topicTemplate,
  );
}

function ensureOutputNodes(
  graph: NonNullable<SaveDataConfigRequest['visualGraph']>,
  topicTemplate: string,
) {
  const inputNodes = graph.nodes.filter((node) => node.kind === 'point' || node.kind === 'algorithm');
  const outputNodes = [
    { kind: 'json' as const, label: 'JSON Payload', nodeId: 'json-payload', refId: null, x: 520, y: 96 },
    { kind: 'mqtt' as const, label: 'MQTT Topic', nodeId: 'mqtt-output', refId: topicTemplate || null, x: 720, y: 96 },
  ];
  const nodes = [...inputNodes, ...outputNodes];
  const edges = [
    ...inputNodes.map((node) => ({
      edgeId: `${node.nodeId}-to-json`,
      from: node.nodeId,
      to: 'json-payload',
    })),
    { edgeId: 'json-to-mqtt', from: 'json-payload', to: 'mqtt-output' },
  ];
  return { edges, nodes };
}

function nodeKindText(kind: DataConfigGraphNodeKind) {
  if (kind === 'point') return '点位';
  if (kind === 'algorithm') return '算法';
  if (kind === 'json') return 'JSON';
  return 'MQTT';
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
