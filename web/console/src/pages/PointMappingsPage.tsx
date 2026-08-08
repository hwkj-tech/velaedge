import { useEffect, useMemo, useState } from 'react';
import { Edit3, FileInput, Plus, ShieldCheck, Trash2, X } from 'lucide-react';

import type {
  CreatePointMappingRequest,
  EdgeNodeResponse,
  ManagementActionResponse,
  PointMappingResponse,
  SavePointMappingRequest,
} from '../api/types';
import { DataTable, type DataTableColumn } from '../components/DataTable';
import { Drawer } from '../components/Drawer';
import { Modal } from '../components/Modal';
import { displayError } from '../utils/errors';
import './PointMappingsPage.css';

const emptyPoint: PointMappingResponse = {
    edgeId: '',
    pointId: '',
    pointName: '',
    deviceId: '',
    deviceModel: '',
    semanticTelemetry: '',
    protocol: '',
    connection: '',
    address: 'holding_register:0',
    valueType: 'float32',
    readWrite: 'read',
    unit: '-',
    scale: '1',
    interval: '1000ms',
    range: '-',
    qualityRule: '',
    status: '停用',
};

const emptyPointSet = buildPointSets([emptyPoint])[0];
const emptyEdge: EdgeNodeResponse = {
  edgeId: '', displayName: '未选择边端', site: '-', runtimeId: '-', status: '未接入',
  resources: '-', heartbeat: '-', capabilities: [],
};

interface PointSet {
  connection: string;
  deviceId: string;
  edgeId: string;
  interval: string;
  pointCount: number;
  points: PointMappingResponse[];
  protocol: string;
  setId: string;
  setName: string;
  status: string;
}

interface CreatePointSetForm {
  connectionId: string;
  deviceId: string;
  intervalMs: number;
  points: Array<{
    addressKind: string;
    addressValue: string;
    intervalMs: number;
    pointId: string;
    semanticId: string;
    unit: string;
    valueType: string;
  }>;
  setName: string;
}

export function PointMappingsPage({
  edges = [],
  embedded = false,
  mode = 'configure',
  onCreatePoint,
  onDeletePoint,
  onImportPoints,
  onSavePoint,
  onValidateDraft,
  points = [],
  selectedEdgeId = edges[0]?.edgeId ?? '',
}: {
  edges?: EdgeNodeResponse[];
  embedded?: boolean;
  mode?: 'configure' | 'list';
  onCreatePoint?: (
    edgeId: string,
    request: CreatePointMappingRequest,
  ) => Promise<PointMappingResponse> | PointMappingResponse;
  onDeletePoint?: (edgeId: string, pointId: string) => Promise<void> | void;
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
  const pointSets = useMemo(() => buildPointSets(points), [points]);
  const [selectedPointSetId, setSelectedPointSetId] = useState(
    () => pointSets[0]?.setId ?? '',
  );
  const selectedPointSet =
    pointSets.find((set) => set.setId === selectedPointSetId) ??
    pointSets[0] ??
    emptyPointSet;
  const [selectedPointId, setSelectedPointId] = useState(
    () => selectedPointSet.points[0]?.pointId ?? '',
  );
  const selectedPoint =
    selectedPointSet.points.find((point) => point.pointId === selectedPointId) ??
    selectedPointSet.points[0] ??
    emptyPoint;
  const [form, setForm] = useState(() => pointToEditorForm(selectedPoint));
  const [saveState, setSaveState] = useState<'idle' | 'saving' | 'saved' | 'error'>(
    'idle',
  );
  const [toolbarMessage, setToolbarMessage] = useState('');
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [editDialogOpen, setEditDialogOpen] = useState(false);
  const [createForm, setCreateForm] = useState<CreatePointSetForm>({
    connectionId: '',
    deviceId: '',
    intervalMs: 1000,
    points: [
      {
        addressKind: 'holding_register',
        addressValue: '40001',
        intervalMs: 1000,
        pointId: '',
        semanticId: '',
        unit: '-',
        valueType: 'float32',
      },
      {
        addressKind: 'coil',
        addressValue: '00001',
        intervalMs: 1000,
        pointId: '',
        semanticId: '',
        unit: '-',
        valueType: 'bool',
      },
    ],
    setName: '新点位集',
  });
  const [actionState, setActionState] = useState<
    'idle' | 'importing' | 'validating' | 'creating'
  >('idle');
  const isConfigureMode = mode === 'configure';
  const activeEdge =
    edges.find((edge) => edge.edgeId === selectedEdgeId) ?? edges[0] ?? emptyEdge;

  useEffect(() => {
    setForm(pointToEditorForm(selectedPoint));
    setSaveState((current) =>
      current === 'saving' || current === 'saved' ? current : 'idle',
    );
  }, [selectedPoint]);

  useEffect(() => {
    if (
      pointSets.length > 0 &&
      !pointSets.some((pointSet) => pointSet.setId === selectedPointSetId)
    ) {
      setSelectedPointSetId(pointSets[0].setId);
      setSelectedPointId(pointSets[0].points[0]?.pointId ?? '');
    }
  }, [pointSets, selectedPointSetId]);

  const handleSave = async () => {
    const request = formToSaveRequest(form);
    setSaveState('saving');

    try {
      await onSavePoint?.(selectedEdgeId, selectedPoint.pointId, request);
      setSaveState('saved');
    } catch (error) {
      setSaveState('error');
      setToolbarMessage(`保存点位失败：${displayError(error)}`);
    }
  };

  const handleImportPoints = async () => {
    setActionState('importing');
    setToolbarMessage('');

    try {
      const result = await onImportPoints?.(selectedEdgeId);
      setToolbarMessage(result?.message ?? '批量导入任务已准备');
    } catch (error) {
      setToolbarMessage(`批量导入失败：${displayError(error)}`);
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
    } catch (error) {
      setToolbarMessage(`点位配置校验失败：${displayError(error)}`);
    } finally {
      setActionState('idle');
    }
  };

  const handleCreatePointSet = async () => {
    setActionState('creating');
    setToolbarMessage('');

    try {
      const validRows = createForm.points.filter(
        (point) => point.pointId.trim() && point.semanticId.trim(),
      );
      if (validRows.length === 0) {
        setToolbarMessage('请至少填写 1 个点位');
        return;
      }
      const createdPoints = [];
      for (const point of validRows) {
        const created = await onCreatePoint?.(selectedEdgeId, {
          addressKind: point.addressKind,
          addressValue: point.addressValue,
          connectionId: createForm.connectionId,
          deviceId: createForm.deviceId,
          intervalMs: Number(point.intervalMs ?? createForm.intervalMs) || 1000,
          pointId: point.pointId,
          semanticId: point.semanticId,
          unit: point.unit || '-',
          valueType: point.valueType,
        });
        if (created) createdPoints.push(created);
      }
      setToolbarMessage(
        createdPoints.length > 0
          ? `已创建点位集 ${createForm.setName}，包含 ${createdPoints.length} 个点位`
          : `已创建点位集 ${createForm.setName}`,
      );
      setCreateDialogOpen(false);
    } catch (error) {
      setToolbarMessage(`创建点位集失败：${displayError(error)}`);
    } finally {
      setActionState('idle');
    }
  };

  const handleDeletePoint = async (pointId: string) => {
    setToolbarMessage('');

    try {
      await onDeletePoint?.(selectedEdgeId, pointId);
      setToolbarMessage(`已删除点位 ${pointId}`);
      setEditDialogOpen(false);
    } catch (error) {
      setToolbarMessage(`删除点位失败：${displayError(error, '请先解除采集任务、数据上报或算法引用')}`);
    }
  };

  const columns = pointSetColumns(
    selectedPointSet.setId,
    (pointSetId) => {
      const pointSet = pointSets.find((item) => item.setId === pointSetId);
      setSelectedPointSetId(pointSetId);
      setSelectedPointId(pointSet?.points[0]?.pointId ?? '');
      setEditDialogOpen(true);
    },
    (pointSetId) => {
      const pointSet = pointSets.find((item) => item.setId === pointSetId);
      pointSet?.points.forEach((point) => {
        void handleDeletePoint(point.pointId);
      });
    },
    isConfigureMode,
  );

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>点位集管理</h2>
          <p>把同一设备或同一采集通道的一批点位定义成点位集，产品侧直接绑定点位集复用。</p>
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
                disabled={actionState === 'importing' || !selectedEdgeId}
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
                disabled={actionState === 'validating' || !selectedEdgeId}
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
                disabled={actionState === 'creating' || !selectedEdgeId}
                onClick={() => setCreateDialogOpen(true)}
                type="button"
              >
                <Plus size={15} aria-hidden="true" />
                新建点位集
              </button>
            </>
          ) : null}
        </div>
      </section>

      {createDialogOpen ? (
        <Modal onClose={() => setCreateDialogOpen(false)}>
          <form
            aria-labelledby="point-create-dialog-title"
            className="modal-panel point-set-create-modal"
            onSubmit={(event) => {
              event.preventDefault();
              void handleCreatePointSet();
            }}
            role="dialog"
          >
            <div className="modal-header">
              <div>
                <h3 id="point-create-dialog-title">新建点位集</h3>
                <p>一次录入同一设备或同一连接下的多个采集点位，产品绑定后可直接复用。</p>
              </div>
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
                <span>点位集名称</span>
                <input
                  aria-label="点位集名称"
                  required
                  value={createForm.setName}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      setName: event.target.value,
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
                <span>默认采集周期(ms)</span>
                <input
                  aria-label="点位集采集周期(ms)"
                  min="100"
                  step="100"
                  type="number"
                  value={createForm.intervalMs}
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      intervalMs: Number(event.target.value),
                    }))
                  }
                />
              </label>
            </div>
            <section className="point-set-editor">
              <div className="point-set-editor-header">
                <h4>点位明细</h4>
                <button
                  className="secondary-button compact"
                  onClick={() =>
                    setCreateForm((current) => ({
                      ...current,
                      points: [
                        ...current.points,
                        {
                          addressKind: 'holding_register',
                          addressValue: '',
                          intervalMs: current.intervalMs,
                          pointId: '',
                          semanticId: '',
                          unit: '-',
                          valueType: 'float32',
                        },
                      ],
                    }))
                  }
                  type="button"
                >
                  <Plus size={14} aria-hidden="true" />
                  添加点位
                </button>
              </div>
              <div className="point-set-row point-set-row-header" aria-hidden="true">
                <span>Point ID</span>
                <span>语义遥测</span>
                <span>地址类型</span>
                <span>地址值</span>
                <span>数据类型</span>
                <span>周期(ms)</span>
                <span>单位</span>
                <span></span>
              </div>
              <div className="point-set-rows">
                {createForm.points.map((point, index) => (
                  <div className="point-set-row" key={index}>
                    <input
                      aria-label={`点位 ${index + 1} Point ID`}
                      placeholder="Point ID"
                      required={index === 0}
                      value={point.pointId}
                      onChange={(event) =>
                        setCreateForm((current) => updateCreatePointSetRow(current, index, { pointId: event.target.value }))
                      }
                    />
                    <input
                      aria-label={`点位 ${index + 1} 语义遥测`}
                      placeholder="语义遥测"
                      required={index === 0}
                      value={point.semanticId}
                      onChange={(event) =>
                        setCreateForm((current) => updateCreatePointSetRow(current, index, { semanticId: event.target.value }))
                      }
                    />
                    <select
                      aria-label={`点位 ${index + 1} 地址类型`}
                      value={point.addressKind}
                      onChange={(event) =>
                        setCreateForm((current) => updateCreatePointSetRow(current, index, { addressKind: event.target.value }))
                      }
                    >
                      <option value="holding_register">保持寄存器（数值/可读写）</option>
                      <option value="input_register">输入寄存器（数值/只读）</option>
                      <option value="coil">线圈（开关量）</option>
                      <option value="simulated">模拟点位（测试）</option>
                    </select>
                    <input
                      aria-label={`点位 ${index + 1} 地址值`}
                      placeholder="地址值"
                      required={index === 0}
                      value={point.addressValue}
                      onChange={(event) =>
                        setCreateForm((current) => updateCreatePointSetRow(current, index, { addressValue: event.target.value }))
                      }
                    />
                    <select
                      aria-label={`点位 ${index + 1} 数据类型`}
                      value={point.valueType}
                      onChange={(event) =>
                        setCreateForm((current) => updateCreatePointSetRow(current, index, { valueType: event.target.value }))
                      }
                    >
                      <option value="float32">float32</option>
                      <option value="int64">int64</option>
                      <option value="bool">bool</option>
                      <option value="string">string</option>
                    </select>
                    <input
                      aria-label={`点位 ${index + 1} 采集周期(ms)`}
                      min="100"
                      placeholder="周期"
                      step="100"
                      type="number"
                      value={point.intervalMs ?? createForm.intervalMs}
                      onChange={(event) =>
                        setCreateForm((current) =>
                          updateCreatePointSetRow(current, index, {
                            intervalMs: Number(event.target.value),
                          }),
                        )
                      }
                    />
                    <input
                      aria-label={`点位 ${index + 1} 单位`}
                      placeholder="单位"
                      value={point.unit}
                      onChange={(event) =>
                        setCreateForm((current) => updateCreatePointSetRow(current, index, { unit: event.target.value }))
                      }
                    />
                    <button
                      aria-label={`移除点位 ${index + 1}`}
                      className="danger-button compact"
                      disabled={createForm.points.length <= 1}
                      onClick={() =>
                        setCreateForm((current) => ({
                          ...current,
                          points: current.points.filter((_, rowIndex) => rowIndex !== index),
                        }))
                      }
                      type="button"
                    >
                      <Trash2 size={14} aria-hidden="true" />
                    </button>
                  </div>
                ))}
              </div>
            </section>
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
        </Modal>
      ) : null}

      <div className={isConfigureMode ? 'point-config-layout' : 'point-config-layout list-only'}>
        <section className="panel point-table-panel">
          <div className="panel-header">
            <h3>点位集列表</h3>
            <span>
              {activeEdge.displayName} · {pointSets.length} 个点位集 · {points.length} 个点位
            </span>
          </div>
          <DataTable
            ariaLabel="点位集分页"
            columns={columns}
            getRowKey={(row) => row.setId}
            pageSize={10}
            rows={pointSets}
          />
        </section>

        {isConfigureMode && editDialogOpen ? (
          <Drawer
          onClose={() => setEditDialogOpen(false)}
          subtitle="保存并通过校验后自动同步到边端 runtime"
          title={`点位集 ${selectedPointSet.setName}`}
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
          <section className="point-set-summary-strip">
            <div>
              <span>设备</span>
              <strong>{selectedPointSet.deviceId}</strong>
            </div>
            <div>
              <span>连接</span>
              <strong>{selectedPointSet.connection}</strong>
            </div>
            <div>
              <span>协议</span>
              <strong>{selectedPointSet.protocol}</strong>
            </div>
            <div>
              <span>点位</span>
              <strong>{selectedPointSet.pointCount} 个</strong>
            </div>
            <div>
              <span>默认周期</span>
              <strong>{selectedPointSet.interval}</strong>
            </div>
          </section>
          <div className="point-detail-layout">
            <aside className="point-list-panel">
              <div className="point-panel-heading">
                <div>
                  <h4>集合内点位</h4>
                  <p>选择一个点位后在右侧修改地址与采集参数。</p>
                </div>
                <span>{selectedPointSet.pointCount}</span>
              </div>
              <div className="point-set-detail-table">
                {selectedPointSet.points.map((point) => (
                  <button
                    aria-label={`选择点位 ${point.pointId}`}
                    className={point.pointId === selectedPoint.pointId ? 'point-set-detail-row active' : 'point-set-detail-row'}
                    key={point.pointId}
                    onClick={() => {
                      setSelectedPointId(point.pointId);
                      setForm(pointToEditorForm(point));
                    }}
                    type="button"
                  >
                    <strong>{point.pointId}</strong>
                    <span>{point.semanticTelemetry}</span>
                    <small>{point.address}</small>
                  </button>
                ))}
              </div>
            </aside>
            <section className="point-edit-panel">
              <div className="point-edit-heading">
                <div>
                  <span>当前点位</span>
                  <h4>{selectedPoint.pointId}</h4>
                  <p>{selectedPoint.semanticTelemetry}</p>
                </div>
                <span className="tag ok">{selectedPoint.status}</span>
              </div>
              <div className="point-meta-grid">
                <div>
                  <span>显示名称</span>
                  <strong>{selectedPoint.pointName}</strong>
                </div>
                <div>
                  <span>数据类型</span>
                  <strong>{selectedPoint.valueType}</strong>
                </div>
                <div>
                  <span>读写</span>
                  <strong>{selectedPoint.readWrite}</strong>
                </div>
                <div>
                  <span>缩放</span>
                  <strong>{selectedPoint.scale}</strong>
                </div>
              </div>
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
                          readWrite: isReadOnlyAddressKind(event.target.value)
                            ? 'read'
                            : current.readWrite,
                        }))
                      }
                    >
                      <option value="holding_register">保持寄存器（数值/可读写）</option>
                      <option value="input_register">输入寄存器（数值/只读）</option>
                      <option value="coil">线圈（开关量）</option>
                      <option value="discrete_input">离散输入（开关量/只读）</option>
                      <option value="node_id">节点 ID（OPC UA）</option>
                      <option value="topic">Topic（订阅地址）</option>
                      <option value="simulated">模拟点位（测试）</option>
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
                  <label className="editor-control">
                    <span>访问权限</span>
                    <select
                      disabled={isReadOnlyAddressKind(form.addressKind)}
                      value={isReadOnlyAddressKind(form.addressKind) ? 'read' : form.readWrite}
                      onChange={(event) =>
                        setForm((current) => ({
                          ...current,
                          readWrite: event.target.value as EditorForm['readWrite'],
                        }))
                      }
                    >
                      <option value="read">只读（仅采集）</option>
                      <option value="read_write">读写（采集与指令）</option>
                      <option value="write">只写（仅指令）</option>
                    </select>
                  </label>
                </div>
              </section>
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
                  ['超时', '800ms'],
                  ['重试', '2 次'],
                  ['死区', '0.02'],
                  ['质量规则', selectedPoint.qualityRule],
                  ['数值范围', selectedPoint.range],
                  ['发布周期', `${formToSaveRequest(form).intervalMs}ms`],
                ]}
                title="治理策略"
              />
            </section>
          </div>
          </Drawer>
        ) : null}
      </div>
    </div>
  );
}

function buildPointSets(points: PointMappingResponse[]): PointSet[] {
  const grouped = new Map<string, PointMappingResponse[]>();
  points.forEach((point) => {
    const setId = `${point.edgeId}:${point.deviceId}:${point.connection}`;
    grouped.set(setId, [...(grouped.get(setId) ?? []), point]);
  });

  return Array.from(grouped.entries()).map(([setId, setPoints]) => {
    const first = setPoints[0];
    return {
      connection: first.connection,
      deviceId: first.deviceId,
      edgeId: first.edgeId,
      interval: dominantValue(setPoints.map((point) => point.interval)),
      pointCount: setPoints.length,
      points: setPoints,
      protocol: first.protocol,
      setId,
      setName: `${first.deviceId} / ${first.connection}`,
      status: setPoints.every((point) => point.status === '启用') ? '启用' : '部分启用',
    };
  });
}

function dominantValue(values: string[]) {
  const counts = new Map<string, number>();
  values.forEach((value) => counts.set(value, (counts.get(value) ?? 0) + 1));
  return Array.from(counts.entries()).sort((left, right) => right[1] - left[1])[0]?.[0] ?? '-';
}

function updateCreatePointSetRow(
  current: CreatePointSetForm,
  index: number,
  patch: Partial<CreatePointSetForm['points'][number]>,
): CreatePointSetForm {
  return {
    ...current,
    points: current.points.map((point, rowIndex) =>
      rowIndex === index ? { ...point, ...patch } : point,
    ),
  };
}

function pointSetColumns(
  selectedPointSetId: string,
  onSelectPointSet: (pointSetId: string) => void,
  onDeletePointSet: (pointSetId: string) => void,
  isConfigureMode: boolean,
): Array<DataTableColumn<PointSet>> {
  return [
    {
      key: 'setName',
      header: '点位集',
      width: '180px',
      render: (row) =>
        isConfigureMode ? (
          <button
            aria-label={`查看点位集 ${row.setName}`}
            aria-pressed={row.setId === selectedPointSetId}
            className="point-id-button"
            onClick={() => onSelectPointSet(row.setId)}
            type="button"
          >
            {row.setName}
          </button>
        ) : (
          row.setName
        ),
    },
    { key: 'deviceId', header: '设备', width: '110px', render: (row) => row.deviceId },
    { key: 'protocol', header: '协议', width: '110px', render: (row) => row.protocol },
    { key: 'connection', header: '连接', width: '150px', render: (row) => row.connection },
    { key: 'pointCount', header: '点位数', width: '90px', render: (row) => `${row.pointCount} 个` },
    { key: 'interval', header: '周期', width: '90px', render: (row) => row.interval },
    {
      key: 'preview',
      header: '点位预览',
      width: '240px',
      render: (row) => row.points.slice(0, 3).map((point) => point.pointId).join(', '),
    },
    {
      key: 'status',
      header: '状态',
      width: '90px',
      render: (row) => <span className="tag ok">{row.status}</span>,
    },
    ...(isConfigureMode
      ? [
          {
            key: 'actions',
            header: '操作',
            width: '110px',
            render: (row: PointSet) => (
              <div className="row-actions">
                <button
                  aria-label={`修改点位集 ${row.setName}`}
                  className="secondary-button compact"
                  onClick={() => onSelectPointSet(row.setId)}
                  type="button"
                >
                  <Edit3 size={14} aria-hidden="true" />
                  修改
                </button>
                <button
                  aria-label={`删除点位集 ${row.setName}`}
                  className="danger-button compact"
                  onClick={() => onDeletePointSet(row.setId)}
                  type="button"
                >
                  <Trash2 size={14} aria-hidden="true" />
                  删除
                </button>
              </div>
            ),
          },
        ]
      : []),
  ];
}

interface EditorForm {
  addressKind: string;
  addressValue: string;
  intervalMs: string;
  readWrite: 'read' | 'read_write' | 'write';
  unit: string;
}

function pointToEditorForm(point: PointMappingResponse): EditorForm {
  const address = splitAddress(point.address);

  return {
    addressKind: address.kind,
    addressValue: address.value,
    intervalMs: String(parseIntervalMs(point.interval)),
    readWrite: normalizePointAccess(point.readWrite),
    unit: point.unit === '-' ? '' : point.unit,
  };
}

function formToSaveRequest(form: EditorForm): SavePointMappingRequest {
  return {
    addressKind: form.addressKind.trim() || 'holding_register',
    addressValue: form.addressValue.trim(),
    intervalMs: Math.max(Number.parseInt(form.intervalMs, 10) || 1000, 100),
    readWrite: isReadOnlyAddressKind(form.addressKind) ? 'read' : form.readWrite,
    unit: form.unit.trim() || '-',
  };
}

function isReadOnlyAddressKind(kind: string): boolean {
  return kind === 'input_register' || kind === 'discrete_input';
}

function normalizePointAccess(value: string): EditorForm['readWrite'] {
  if (value === 'read_write' || value === 'write') return value;
  return 'read';
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
