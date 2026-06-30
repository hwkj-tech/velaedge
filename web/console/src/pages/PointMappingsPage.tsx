import { useEffect, useState } from 'react';
import { FileInput, Plus, ShieldCheck, X } from 'lucide-react';

import type {
  CreatePointMappingRequest,
  EdgeNodeResponse,
  ManagementActionResponse,
  PointMappingResponse,
  SavePointMappingRequest,
} from '../api/types';
import { DataTable, type DataTableColumn } from '../components/DataTable';
import { Drawer } from '../components/Drawer';
import './PointMappingsPage.css';

const fallbackPoints: PointMappingResponse[] = [
  {
    edgeId: 'edge-dev',
    pointId: 'pressure',
    pointName: '泵出口压力',
    deviceId: 'pump-1',
    deviceModel: 'pump@v1',
    semanticTelemetry: 'pump.pressure',
    protocol: 'Modbus TCP',
    connection: 'modbus-line-a',
    address: 'holding_register:40001',
    valueType: 'float32',
    readWrite: 'read',
    unit: 'MPa',
    scale: '0.1',
    interval: '1000ms',
    range: '0-20',
    qualityRule: 'timeout->bad',
    status: '启用',
  },
  {
    edgeId: 'edge-dev',
    pointId: 'running',
    pointName: '运行状态',
    deviceId: 'pump-1',
    deviceModel: 'pump@v1',
    semanticTelemetry: 'pump.running',
    protocol: 'Modbus TCP',
    connection: 'modbus-line-a',
    address: 'coil:00001',
    valueType: 'bool',
    readWrite: 'read',
    unit: '-',
    scale: '1',
    interval: '1000ms',
    range: '-',
    qualityRule: 'stale->bad',
    status: '启用',
  },
];

const fallbackEdges: EdgeNodeResponse[] = [
  {
    edgeId: 'edge-dev',
    displayName: '研发实验室边端',
    site: '研发/实验室',
    runtimeId: 'runtime-dev',
    status: '健康',
    resources: '18.5% / 42% / 61%',
    heartbeat: '8 秒前',
    capabilities: ['protocol:modbus-tcp'],
  },
];

export function PointMappingsPage({
  edges = fallbackEdges,
  embedded = false,
  mode = 'configure',
  onCreatePoint,
  onImportPoints,
  onSavePoint,
  onValidateDraft,
  points = fallbackPoints,
  selectedEdgeId = edges[0]?.edgeId ?? 'edge-dev',
}: {
  edges?: EdgeNodeResponse[];
  embedded?: boolean;
  mode?: 'configure' | 'list';
  onCreatePoint?: (
    edgeId: string,
    request: CreatePointMappingRequest,
  ) => Promise<PointMappingResponse> | PointMappingResponse;
  onImportPoints?: (
    edgeId: string,
  ) => Promise<ManagementActionResponse> | ManagementActionResponse;
  onSavePoint?: (
    edgeId: string,
    pointId: string,
    request: SavePointMappingRequest,
  ) => Promise<void> | void;
  onValidateDraft?: (
    edgeId: string,
  ) => Promise<ManagementActionResponse> | ManagementActionResponse;
  points?: PointMappingResponse[];
  selectedEdgeId?: string;
}) {
  const [selectedPointId, setSelectedPointId] = useState(
    () => points[0]?.pointId ?? fallbackPoints[0].pointId,
  );
  const selectedPoint =
    points.find((point) => point.pointId === selectedPointId) ??
    points[0] ??
    fallbackPoints[0];
  const [form, setForm] = useState(() => pointToEditorForm(selectedPoint));
  const [saveState, setSaveState] = useState<'idle' | 'saving' | 'saved' | 'error'>(
    'idle',
  );
  const [toolbarMessage, setToolbarMessage] = useState('');
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [editDialogOpen, setEditDialogOpen] = useState(false);
  const [createForm, setCreateForm] = useState<CreatePointMappingRequest>({
    addressKind: 'holding_register',
    addressValue: '40001',
    connectionId: fallbackPoints[0].connection,
    deviceId: fallbackPoints[0].deviceId,
    intervalMs: 1000,
    pointId: '',
    semanticId: '',
    unit: '-',
    valueType: 'float32',
  });
  const [actionState, setActionState] = useState<
    'idle' | 'importing' | 'validating' | 'creating'
  >('idle');
  const isConfigureMode = mode === 'configure';
  const columns = pointColumns(
    selectedPoint.pointId,
    (pointId) => {
      setSelectedPointId(pointId);
      setEditDialogOpen(true);
    },
    isConfigureMode,
  );
  const activeEdge =
    edges.find((edge) => edge.edgeId === selectedEdgeId) ?? edges[0] ?? fallbackEdges[0];

  useEffect(() => {
    setForm(pointToEditorForm(selectedPoint));
    setSaveState((current) =>
      current === 'saving' || current === 'saved' ? current : 'idle',
    );
  }, [selectedPoint]);

  useEffect(() => {
    if (points.length > 0 && !points.some((point) => point.pointId === selectedPointId)) {
      setSelectedPointId(points[0].pointId);
    }
  }, [points, selectedPointId]);

  const handleSave = async () => {
    const request = formToSaveRequest(form);
    setSaveState('saving');

    try {
      await onSavePoint?.(selectedEdgeId, selectedPoint.pointId, request);
      setSaveState('saved');
    } catch {
      setSaveState('error');
    }
  };

  const handleImportPoints = async () => {
    setActionState('importing');
    setToolbarMessage('');

    try {
      const result = await onImportPoints?.(selectedEdgeId);
      setToolbarMessage(result?.message ?? '批量导入任务已准备');
    } catch {
      setToolbarMessage('批量导入失败');
    } finally {
      setActionState('idle');
    }
  };

  const handleValidateDraft = async () => {
    setActionState('validating');
    setToolbarMessage('');

    try {
      const result = await onValidateDraft?.(selectedEdgeId);
      setToolbarMessage(
        result?.status ? `点位配置校验 ${result.status}` : '点位配置校验已完成',
      );
    } catch {
      setToolbarMessage('点位配置校验失败');
    } finally {
      setActionState('idle');
    }
  };

  const handleCreatePoint = async () => {
    setActionState('creating');
    setToolbarMessage('');

    try {
      const created = await onCreatePoint?.(selectedEdgeId, {
        ...createForm,
        intervalMs: Number(createForm.intervalMs) || 1000,
      });
      setToolbarMessage(
        created ? `已创建点位 ${created.pointId}` : '已创建点位',
      );
      setCreateDialogOpen(false);
    } catch {
      setToolbarMessage('创建点位失败');
    } finally {
      setActionState('idle');
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>语义点位到协议地址</h2>
          <p>
            点位在云端集中配置和校验，发布后由边端 runtime 按协议适配器执行采集、缓存和质量规则。
          </p>
        </div>
        <div className="toolbar">
          {toolbarMessage ? (
            <span className="toolbar-status" role="status">
              {toolbarMessage}
            </span>
          ) : null}
          {isConfigureMode ? (
            <>
              <button
                className="secondary-button"
                disabled={actionState === 'importing'}
                onClick={() => {
                  void handleImportPoints();
                }}
                type="button"
              >
                <FileInput size={15} aria-hidden="true" />
                {actionState === 'importing' ? '导入中' : '批量导入'}
              </button>
              <button
                className="secondary-button"
                disabled={actionState === 'validating'}
                onClick={() => {
                  void handleValidateDraft();
                }}
                type="button"
              >
                <ShieldCheck size={15} aria-hidden="true" />
                {actionState === 'validating' ? '校验中' : '校验配置'}
              </button>
              <button
                className="primary-button"
                disabled={actionState === 'creating'}
                onClick={() => setCreateDialogOpen(true)}
                type="button"
              >
                <Plus size={15} aria-hidden="true" />
                新建点位
              </button>
            </>
          ) : null}
        </div>
      </section>

      {createDialogOpen ? (
        <div className="modal-backdrop">
          <form
            aria-labelledby="point-create-dialog-title"
            className="modal-panel"
            onSubmit={(event) => {
              event.preventDefault();
              void handleCreatePoint();
            }}
            role="dialog"
          >
            <div className="modal-header">
              <h3 id="point-create-dialog-title">新建点位</h3>
              <button
                aria-label="关闭"
                className="icon-button"
                onClick={() => setCreateDialogOpen(false)}
                type="button"
              >
                <X size={16} aria-hidden="true" />
              </button>
            </div>
            <div className="form-grid">
              <label>
                <span>Point ID</span>
                <input
                  aria-label="新建 Point ID"
                  required
                  value={createForm.pointId ?? ''}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      pointId: event.target.value,
                    }))
                  }
                />
              </label>
              <label>
                <span>设备 ID</span>
                <input
                  aria-label="新建设备 ID"
                  required
                  value={createForm.deviceId ?? ''}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      deviceId: event.target.value,
                    }))
                  }
                />
              </label>
              <label>
                <span>语义遥测</span>
                <input
                  aria-label="新建语义遥测"
                  required
                  value={createForm.semanticId ?? ''}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      semanticId: event.target.value,
                    }))
                  }
                />
              </label>
              <label>
                <span>连接实例</span>
                <input
                  aria-label="新建连接实例"
                  required
                  value={createForm.connectionId ?? ''}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      connectionId: event.target.value,
                    }))
                  }
                />
              </label>
              <label>
                <span>地址类型</span>
                <select
                  aria-label="新建地址类型"
                  value={createForm.addressKind ?? 'holding_register'}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      addressKind: event.target.value,
                    }))
                  }
                >
                  <option value="holding_register">holding_register</option>
                  <option value="input_register">input_register</option>
                  <option value="coil">coil</option>
                  <option value="simulated">simulated</option>
                </select>
              </label>
              <label>
                <span>地址值</span>
                <input
                  aria-label="新建地址值"
                  required
                  value={createForm.addressValue ?? ''}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      addressValue: event.target.value,
                    }))
                  }
                />
              </label>
              <label>
                <span>数据类型</span>
                <select
                  aria-label="新建数据类型"
                  value={createForm.valueType ?? 'float32'}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      valueType: event.target.value,
                    }))
                  }
                >
                  <option value="float32">float32</option>
                  <option value="int64">int64</option>
                  <option value="bool">bool</option>
                  <option value="string">string</option>
                </select>
              </label>
              <label>
                <span>采集周期(ms)</span>
                <input
                  aria-label="新建采集周期(ms)"
                  min="100"
                  step="100"
                  type="number"
                  value={createForm.intervalMs ?? 1000}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      intervalMs: Number(event.target.value),
                    }))
                  }
                />
              </label>
              <label>
                <span>单位</span>
                <input
                  aria-label="新建单位"
                  value={createForm.unit ?? ''}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      unit: event.target.value,
                    }))
                  }
                />
              </label>
            </div>
            <div className="modal-actions">
              <button
                className="secondary-button"
                onClick={() => setCreateDialogOpen(false)}
                type="button"
              >
                取消
              </button>
              <button
                className="primary-button"
                disabled={actionState === 'creating'}
                type="submit"
              >
                {actionState === 'creating' ? '保存中' : '保存'}
              </button>
            </div>
          </form>
        </div>
      ) : null}

      <div className={isConfigureMode ? 'point-config-layout' : 'point-config-layout list-only'}>
        <section className="panel point-table-panel">
          <div className="panel-header">
            <h3>点位配置表</h3>
            <span>
              {activeEdge.displayName} · {points.length} 个启用点位
            </span>
          </div>
          <DataTable
            ariaLabel="点位分页"
            columns={columns}
            getRowKey={(row) => row.pointId}
            pageSize={10}
            rows={points}
          />
        </section>

        {isConfigureMode && editDialogOpen ? (
          <Drawer
          onClose={() => setEditDialogOpen(false)}
          subtitle="保存后进入待发布配置，发布后边端 runtime 执行"
          title={`编辑点位 ${selectedPoint.pointId}`}
          footer={
            <>
              <span className={`editor-status ${saveState}`} role="status">
                {saveStatusText(saveState)}
              </span>
              <button
                className="secondary-button"
                onClick={() => {
                  setForm(pointToEditorForm(selectedPoint));
                  setSaveState('idle');
                  setEditDialogOpen(false);
                }}
                type="button"
              >
                取消
              </button>
              <button
                className="primary-button"
                disabled={saveState === 'saving'}
                onClick={handleSave}
                type="button"
              >
                {saveState === 'saving' ? '保存中' : '保存'}
              </button>
            </>
          }
        >
          <DrawerSection
            fields={[
              ['Point ID', selectedPoint.pointId],
              ['显示名称', selectedPoint.pointName],
              ['设备 ID', selectedPoint.deviceId],
              ['设备模型', selectedPoint.deviceModel],
              ['语义遥测', selectedPoint.semanticTelemetry],
              ['启用状态', selectedPoint.status],
            ]}
            title="基础信息"
          />
          <DrawerSection
            fields={[
              ['协议类型', selectedPoint.protocol],
              ['连接实例', selectedPoint.connection],
              ['数据类型', selectedPoint.valueType],
              ['读写类型', selectedPoint.readWrite],
              ['缩放系数', selectedPoint.scale],
              ['偏移量', '0'],
            ]}
            title="协议映射"
          />
          <section className="drawer-section">
            <h4>地址配置</h4>
            <div className="editor-grid">
              <label className="editor-control">
                <span>地址类型</span>
                <select
                  value={form.addressKind}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      addressKind: event.target.value,
                    }))
                  }
                >
                  <option value="holding_register">holding_register</option>
                  <option value="input_register">input_register</option>
                  <option value="coil">coil</option>
                  <option value="node_id">node_id</option>
                  <option value="topic">topic</option>
                  <option value="simulated">simulated</option>
                </select>
              </label>
              <label className="editor-control">
                <span>地址值</span>
                <input
                  value={form.addressValue}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      addressValue: event.target.value,
                    }))
                  }
                />
              </label>
            </div>
          </section>
          <DrawerSection
            fields={[
              ['超时', '800ms'],
              ['重试次数', '2'],
              ['死区', '0.02'],
              ['缓存策略', 'local-first'],
            ]}
            title="采集策略"
          />
          <section className="drawer-section">
            <h4>采集参数</h4>
            <div className="editor-grid">
              <label className="editor-control">
                <span>采集周期(ms)</span>
                <input
                  min="100"
                  step="100"
                  type="number"
                  value={form.intervalMs}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      intervalMs: event.target.value,
                    }))
                  }
                />
              </label>
              <label className="editor-control">
                <span>单位</span>
                <input
                  value={form.unit}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      unit: event.target.value,
                    }))
                  }
                />
              </label>
            </div>
          </section>
          <DrawerSection
            fields={[
              ['采集周期', `${formToSaveRequest(form).intervalMs}ms`],
              ['数值范围', selectedPoint.range],
              ['精度', '2'],
              ['质量规则', selectedPoint.qualityRule],
              ['告警规则', 'pressure-high'],
            ]}
            title="数据治理"
          />
          </Drawer>
        ) : null}
      </div>
    </div>
  );
}

function pointColumns(
  selectedPointId: string,
  onSelectPoint: (pointId: string) => void,
  isConfigureMode: boolean,
): Array<DataTableColumn<PointMappingResponse>> {
  return [
    {
      key: 'pointId',
      header: 'Point ID',
      width: '110px',
      render: (row) =>
        isConfigureMode ? (
          <button
            aria-label={`选择点位 ${row.pointId}`}
            aria-pressed={row.pointId === selectedPointId}
            className="point-id-button"
            onClick={() => onSelectPoint(row.pointId)}
            type="button"
          >
            {row.pointId}
          </button>
        ) : (
          row.pointId
        ),
    },
    {
      key: 'address',
      header: '地址 / NodeId / Topic',
      width: '180px',
      render: (row) => row.address,
    },
    { key: 'deviceId', header: '设备', width: '90px', render: (row) => row.deviceId },
    { key: 'protocol', header: '协议', width: '110px', render: (row) => row.protocol },
    { key: 'connection', header: '连接', width: '130px', render: (row) => row.connection },
    {
      key: 'semanticTelemetry',
      header: '语义遥测',
      width: '130px',
      render: (row) => row.semanticTelemetry,
    },
    { key: 'type', header: '数据类型', width: '90px', render: (row) => row.valueType },
    { key: 'unit', header: '单位', width: '80px', render: (row) => row.unit },
    { key: 'interval', header: '周期', width: '90px', render: (row) => row.interval },
    {
      key: 'status',
      header: '状态',
      width: '90px',
      render: (row) => <span className="tag ok">{row.status}</span>,
    },
  ];
}

interface EditorForm {
  addressKind: string;
  addressValue: string;
  intervalMs: string;
  unit: string;
}

function pointToEditorForm(point: PointMappingResponse): EditorForm {
  const address = splitAddress(point.address);

  return {
    addressKind: address.kind,
    addressValue: address.value,
    intervalMs: String(parseIntervalMs(point.interval)),
    unit: point.unit === '-' ? '' : point.unit,
  };
}

function formToSaveRequest(form: EditorForm): SavePointMappingRequest {
  return {
    addressKind: form.addressKind.trim() || 'holding_register',
    addressValue: form.addressValue.trim(),
    intervalMs: Math.max(Number.parseInt(form.intervalMs, 10) || 1000, 100),
    unit: form.unit.trim() || '-',
  };
}

function splitAddress(address: string): { kind: string; value: string } {
  const separatorIndex = address.indexOf(':');
  if (separatorIndex === -1) {
    return { kind: 'holding_register', value: address };
  }

  return {
    kind: address.slice(0, separatorIndex),
    value: address.slice(separatorIndex + 1),
  };
}

function parseIntervalMs(interval: string): number {
  return Number.parseInt(interval.replace(/[^\d]/g, ''), 10) || 1000;
}

function saveStatusText(saveState: 'idle' | 'saving' | 'saved' | 'error') {
  switch (saveState) {
    case 'saving':
      return '保存中';
    case 'saved':
      return '已保存';
    case 'error':
      return '保存失败';
    case 'idle':
      return '';
  }
}

function DrawerSection({
  fields,
  title,
}: {
  fields: Array<[string, string]>;
  title: string;
}) {
  return (
    <section className="drawer-section">
      <h4>{title}</h4>
      <div className="editor-grid">
        {fields.map(([label, value]) => (
          <div className="editor-field" key={label}>
            <span>{label}</span>
            <strong>{value}</strong>
          </div>
        ))}
      </div>
    </section>
  );
}
