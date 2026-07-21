import { useMemo, useState } from 'react';
import { Edit3, Plus, Trash2, X } from 'lucide-react';

import type {
  PointSetPointResponse,
  PointSetResponse,
  SavePointSetRequest,
} from '../api/types';
import { DataTable, type DataTableColumn } from '../components/DataTable';
import { Modal } from '../components/Modal';
import { displayError } from '../utils/errors';
import './PointMappingsPage.css';

interface ProjectOption {
  projectId: string;
  name: string;
}

interface PointSetEditorState extends SavePointSetRequest {}

interface CustomSerialFrameSpec {
  offset: number;
  requestChecksum: string;
  requestHex: string;
  responseChecksum: string;
  responsePrefixHex: string;
  scale: number;
  valueEncoding: string;
  valueLength?: number;
  valueOffset: number;
}

const protocolOptions = [
  ['ModbusRtu', 'Modbus RTU'],
  ['ModbusTcp', 'Modbus TCP'],
  ['Dlt645', 'DL/T 645'],
  ['Iec101', 'IEC 101'],
  ['CustomSerial', '自定义串口'],
  ['Simulated', '模拟协议'],
] as const;

export function PointSetsPage({
  onCreate,
  onDelete,
  onSave,
  pointSets,
  projects,
}: {
  onCreate: (request: SavePointSetRequest) => Promise<PointSetResponse>;
  onDelete: (pointSetId: string) => Promise<void>;
  onSave: (
    pointSetId: string,
    request: SavePointSetRequest,
  ) => Promise<PointSetResponse>;
  pointSets: PointSetResponse[];
  projects: ProjectOption[];
}) {
  const [editor, setEditor] = useState<PointSetEditorState>();
  const [editingId, setEditingId] = useState<string>();
  const [deleteTarget, setDeleteTarget] = useState<PointSetResponse>();
  const [message, setMessage] = useState('');
  const [submitting, setSubmitting] = useState(false);

  const openCreate = () => {
    setEditingId(undefined);
    setEditor(emptyPointSet(projects[0]?.projectId ?? ''));
  };

  const openEdit = (pointSet: PointSetResponse) => {
    setEditingId(pointSet.pointSetId);
    setEditor({
      description: pointSet.description,
      name: pointSet.name,
      pointSetId: pointSet.pointSetId,
      points: pointSet.points.map((point) => ({ ...point, address: { ...point.address } })),
      projectId: pointSet.projectId,
      protocol: pointSet.protocol,
    });
  };

  const submit = async () => {
    if (!editor) return;
    const error = validatePointSet(editor);
    if (error) {
      setMessage(error);
      return;
    }

    setSubmitting(true);
    setMessage('');
    try {
      if (editingId) {
        await onSave(editingId, editor);
        setMessage(`已保存点位集 ${editor.name}`);
      } else {
        await onCreate(editor);
        setMessage(`已创建点位集 ${editor.name}`);
      }
      setEditor(undefined);
      setEditingId(undefined);
    } catch (error) {
      setMessage(`保存点位集失败：${displayError(error)}`);
    } finally {
      setSubmitting(false);
    }
  };

  const confirmDelete = async () => {
    if (!deleteTarget) return;
    setSubmitting(true);
    setMessage('');
    try {
      await onDelete(deleteTarget.pointSetId);
      setMessage(`已删除点位集 ${deleteTarget.name}`);
      setDeleteTarget(undefined);
    } catch (error) {
      setMessage(
        `删除点位集失败：${displayError(error, '请先解除产品版本中的点位集引用')}`,
      );
    } finally {
      setSubmitting(false);
    }
  };

  const projectNames = useMemo(
    () => new Map(projects.map((project) => [project.projectId, project.name])),
    [projects],
  );
  const columns = pointSetColumns(projectNames, openEdit, setDeleteTarget);

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>点位集管理</h2>
          <p>集中维护可复用的协议点位集合，产品版本直接绑定点位集。</p>
        </div>
        <div className="toolbar">
          {message ? <span className="toolbar-status" role="status">{message}</span> : null}
          <button className="primary-button" onClick={openCreate} type="button">
            <Plus aria-hidden="true" size={15} />
            新建点位集
          </button>
        </div>
      </section>

      <section className="panel point-table-panel">
        <div className="panel-header">
          <h3>点位集列表</h3>
          <span>{pointSets.length} 个点位集 · {pointSets.reduce((sum, set) => sum + set.points.length, 0)} 个点位</span>
        </div>
        <DataTable
          ariaLabel="点位集分页"
          columns={columns}
          getRowKey={(row) => row.pointSetId}
          pageSize={10}
          rows={pointSets}
        />
      </section>

      {editor ? (
        <Modal onClose={() => setEditor(undefined)}>
          <form
            aria-labelledby="point-set-editor-title"
            className="modal-panel point-set-create-modal"
            onSubmit={(event) => {
              event.preventDefault();
              void submit();
            }}
            role="dialog"
          >
            <div className="modal-header">
              <div>
                <h3 id="point-set-editor-title">{editingId ? '编辑点位集' : '新建点位集'}</h3>
                <p>点位集属于项目，可被同一项目下的多个产品版本复用。</p>
              </div>
              <button aria-label="关闭" className="icon-button" onClick={() => setEditor(undefined)} type="button">
                <X aria-hidden="true" size={16} />
              </button>
            </div>

            <div className="form-grid">
              <label>
                <span>点位集 ID</span>
                <input
                  aria-label="点位集 ID"
                  disabled={Boolean(editingId)}
                  placeholder="pump-standard-points"
                  value={editor.pointSetId}
                  onChange={(event) => setEditor({ ...editor, pointSetId: event.target.value })}
                />
              </label>
              <label>
                <span>名称</span>
                <input
                  aria-label="点位集名称"
                  value={editor.name}
                  onChange={(event) => setEditor({ ...editor, name: event.target.value })}
                />
              </label>
              <label>
                <span>所属项目</span>
                <select
                  aria-label="所属项目"
                  value={editor.projectId}
                  onChange={(event) => setEditor({ ...editor, projectId: event.target.value })}
                >
                  {projects.map((project) => (
                    <option key={project.projectId} value={project.projectId}>{project.name}</option>
                  ))}
                </select>
              </label>
              <label>
                <span>协议</span>
                <select
                  aria-label="协议"
                  value={editor.protocol}
                  onChange={(event) => setEditor(changePointSetProtocol(editor, event.target.value))}
                >
                  {protocolOptions.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
                </select>
              </label>
              <label className="span-two">
                <span>说明</span>
                <input
                  aria-label="说明"
                  value={editor.description}
                  onChange={(event) => setEditor({ ...editor, description: event.target.value })}
                />
              </label>
            </div>

            <section className="point-set-editor">
              <div className="point-set-editor-header">
                <div>
                  <h4>点位明细</h4>
                  <small>每个点位可独立设置地址、类型、单位和采集周期。</small>
                </div>
                <button
                  className="secondary-button compact"
                  onClick={() => setEditor({ ...editor, points: [...editor.points, emptyPoint()] })}
                  type="button"
                >
                  <Plus aria-hidden="true" size={14} />
                  添加点位
                </button>
              </div>
              {editor.protocol !== 'CustomSerial' ? (
                <div className="point-set-row point-set-row-header" aria-hidden="true">
                  <span>Point ID</span><span>语义 ID</span><span>地址类型</span><span>地址值</span>
                  <span>数据类型</span><span>周期(ms)</span><span>单位</span><span />
                </div>
              ) : null}
              <div className="point-set-rows">
                {editor.points.map((point, index) => (
                  <PointRow
                    index={index}
                    key={`${point.pointId}-${index}`}
                    onChange={(nextPoint) => setEditor({
                      ...editor,
                      points: editor.points.map((item, itemIndex) => itemIndex === index ? nextPoint : item),
                    })}
                    onRemove={() => setEditor({ ...editor, points: editor.points.filter((_, itemIndex) => itemIndex !== index) })}
                    point={point}
                    protocol={editor.protocol}
                    removable={editor.points.length > 1}
                  />
                ))}
              </div>
            </section>

            <div className="modal-actions">
              <button className="secondary-button" onClick={() => setEditor(undefined)} type="button">取消</button>
              <button className="primary-button" disabled={submitting} type="submit">{submitting ? '保存中' : '保存'}</button>
            </div>
          </form>
        </Modal>
      ) : null}

      {deleteTarget ? (
        <Modal onClose={() => setDeleteTarget(undefined)}>
          <section aria-labelledby="delete-point-set-title" className="modal-panel compact-modal" role="dialog">
            <div className="modal-header">
              <div>
                <h3 id="delete-point-set-title">删除点位集</h3>
                <p>将删除“{deleteTarget.name}”及其中 {deleteTarget.points.length} 个点位。</p>
              </div>
              <button aria-label="关闭" className="icon-button" onClick={() => setDeleteTarget(undefined)} type="button">
                <X aria-hidden="true" size={16} />
              </button>
            </div>
            <div className="modal-actions">
              <button className="secondary-button" onClick={() => setDeleteTarget(undefined)} type="button">取消</button>
              <button className="danger-button" disabled={submitting} onClick={() => void confirmDelete()} type="button">确认删除</button>
            </div>
          </section>
        </Modal>
      ) : null}
    </div>
  );
}

function PointRow({
  index,
  onChange,
  onRemove,
  point,
  protocol,
  removable,
}: {
  index: number;
  onChange: (point: PointSetPointResponse) => void;
  onRemove: () => void;
  point: PointSetPointResponse;
  protocol: string;
  removable: boolean;
}) {
  if (protocol === 'CustomSerial') {
    return (
      <CustomSerialPointRow
        index={index}
        onChange={onChange}
        onRemove={onRemove}
        point={point}
        removable={removable}
      />
    );
  }
  return (
    <div className="point-set-row">
      <input aria-label={`点位 ${index + 1} Point ID`} value={point.pointId} onChange={(event) => onChange({ ...point, pointId: event.target.value })} />
      <input aria-label={`点位 ${index + 1} 语义 ID`} value={point.semanticId} onChange={(event) => onChange({ ...point, semanticId: event.target.value })} />
      <select aria-label={`点位 ${index + 1} 地址类型`} value={point.address.kind} onChange={(event) => onChange({ ...point, address: { ...point.address, kind: event.target.value } })}>
        <option value="holding_register">保持寄存器</option>
        <option value="input_register">输入寄存器</option>
        <option value="coil">线圈</option>
        <option value="discrete_input">离散输入</option>
        <option value="dlt645_address">DL/T 645 数据标识</option>
        <option value="iec101_ioa">IEC 101 信息体地址</option>
        <option value="simulated">模拟点位</option>
      </select>
      <input aria-label={`点位 ${index + 1} 地址值`} value={point.address.value} onChange={(event) => onChange({ ...point, address: { ...point.address, value: event.target.value } })} />
      <select aria-label={`点位 ${index + 1} 数据类型`} value={point.valueType} onChange={(event) => onChange({ ...point, valueType: event.target.value })}>
        <option value="float32">float32</option><option value="float64">float64</option>
        <option value="int32">int32</option><option value="int64">int64</option>
        <option value="bool">bool</option><option value="string">string</option>
      </select>
      <input aria-label={`点位 ${index + 1} 采集周期(ms)`} min="100" step="100" type="number" value={point.intervalMs} onChange={(event) => onChange({ ...point, intervalMs: Number(event.target.value) })} />
      <input aria-label={`点位 ${index + 1} 单位`} value={point.unit ?? ''} onChange={(event) => onChange({ ...point, unit: event.target.value || null })} />
      <button aria-label={`移除点位 ${index + 1}`} className="danger-button compact" disabled={!removable} onClick={onRemove} type="button"><Trash2 aria-hidden="true" size={14} /></button>
    </div>
  );
}

function CustomSerialPointRow({
  index,
  onChange,
  onRemove,
  point,
  removable,
}: {
  index: number;
  onChange: (point: PointSetPointResponse) => void;
  onRemove: () => void;
  point: PointSetPointResponse;
  removable: boolean;
}) {
  const frame = parseCustomSerialFrame(point.address.value);
  const updateFrame = (patch: Partial<CustomSerialFrameSpec>) => {
    onChange({
      ...point,
      address: {
        kind: 'custom_serial_frame',
        value: serializeCustomSerialFrame({ ...frame, ...patch }),
      },
    });
  };
  return (
    <section aria-label={`自定义串口点位 ${index + 1}`} className="custom-serial-point-row">
      <div className="custom-serial-point-meta">
        <label><span>Point ID</span><input aria-label={`点位 ${index + 1} Point ID`} value={point.pointId} onChange={(event) => onChange({ ...point, pointId: event.target.value })} /></label>
        <label><span>语义 ID</span><input aria-label={`点位 ${index + 1} 语义 ID`} value={point.semanticId} onChange={(event) => onChange({ ...point, semanticId: event.target.value })} /></label>
        <label><span>数据类型</span><select aria-label={`点位 ${index + 1} 数据类型`} value={point.valueType} onChange={(event) => onChange({ ...point, valueType: event.target.value })}><option value="float32">float32</option><option value="float64">float64</option><option value="int32">int32</option><option value="int64">int64</option><option value="bool">bool</option><option value="string">string</option></select></label>
        <label><span>周期(ms)</span><input aria-label={`点位 ${index + 1} 采集周期(ms)`} min="100" step="100" type="number" value={point.intervalMs} onChange={(event) => onChange({ ...point, intervalMs: Number(event.target.value) })} /></label>
        <label><span>单位</span><input aria-label={`点位 ${index + 1} 单位`} value={point.unit ?? ''} onChange={(event) => onChange({ ...point, unit: event.target.value || null })} /></label>
        <button aria-label={`移除点位 ${index + 1}`} className="danger-button compact" disabled={!removable} onClick={onRemove} type="button"><Trash2 aria-hidden="true" size={14} /></button>
      </div>
      <div className="custom-serial-frame-grid">
        <label className="frame-wide"><span>请求帧 HEX</span><input aria-label={`点位 ${index + 1} 请求帧 HEX`} placeholder="01 03 00 10" value={frame.requestHex} onChange={(event) => updateFrame({ requestHex: event.target.value })} /></label>
        <label><span>请求校验</span><ChecksumSelect ariaLabel={`点位 ${index + 1} 请求校验`} onChange={(requestChecksum) => updateFrame({ requestChecksum })} value={frame.requestChecksum} /></label>
        <label><span>响应校验</span><ChecksumSelect ariaLabel={`点位 ${index + 1} 响应校验`} onChange={(responseChecksum) => updateFrame({ responseChecksum })} value={frame.responseChecksum} /></label>
        <label><span>响应前缀 HEX</span><input aria-label={`点位 ${index + 1} 响应前缀 HEX`} placeholder="可选，如 01 03" value={frame.responsePrefixHex} onChange={(event) => updateFrame({ responsePrefixHex: event.target.value })} /></label>
        <label><span>取值偏移</span><input aria-label={`点位 ${index + 1} 取值偏移`} min="0" type="number" value={frame.valueOffset} onChange={(event) => updateFrame({ valueOffset: Number(event.target.value) })} /></label>
        <label><span>值编码</span><select aria-label={`点位 ${index + 1} 值编码`} value={frame.valueEncoding} onChange={(event) => updateFrame({ valueEncoding: event.target.value, valueLength: event.target.value === 'utf8' ? (frame.valueLength ?? 1) : undefined })}>{customSerialEncodingOptions.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
        <label><span>文本长度</span><input aria-label={`点位 ${index + 1} 文本长度`} disabled={frame.valueEncoding !== 'utf8'} min="1" type="number" value={frame.valueLength ?? ''} onChange={(event) => updateFrame({ valueLength: Number(event.target.value) })} /></label>
        <label><span>缩放</span><input aria-label={`点位 ${index + 1} 缩放`} step="any" type="number" value={frame.scale} onChange={(event) => updateFrame({ scale: Number(event.target.value) })} /></label>
        <label><span>偏置</span><input aria-label={`点位 ${index + 1} 偏置`} step="any" type="number" value={frame.offset} onChange={(event) => updateFrame({ offset: Number(event.target.value) })} /></label>
      </div>
    </section>
  );
}

function ChecksumSelect({ ariaLabel, onChange, value }: { ariaLabel: string; onChange: (value: string) => void; value: string }) {
  return <select aria-label={ariaLabel} onChange={(event) => onChange(event.target.value)} value={value}><option value="none">无</option><option value="sum8">SUM8</option><option value="xor8">XOR8</option><option value="modbus_crc16">Modbus CRC16</option></select>;
}

const customSerialEncodingOptions = [
  ['bool_u8', '布尔 U8'], ['u8', 'U8'], ['i8', 'I8'], ['u16_be', 'U16 大端'],
  ['u16_le', 'U16 小端'], ['i16_be', 'I16 大端'], ['i16_le', 'I16 小端'],
  ['u32_be', 'U32 大端'], ['u32_le', 'U32 小端'], ['i32_be', 'I32 大端'],
  ['i32_le', 'I32 小端'], ['f32_be', 'F32 大端'], ['f32_le', 'F32 小端'],
  ['f64_be', 'F64 大端'], ['f64_le', 'F64 小端'], ['utf8', 'UTF-8 文本'],
] as const;

function pointSetColumns(
  projectNames: Map<string, string>,
  onEdit: (pointSet: PointSetResponse) => void,
  onDelete: (pointSet: PointSetResponse) => void,
): Array<DataTableColumn<PointSetResponse>> {
  return [
    { key: 'name', header: '点位集', width: '220px', render: (row) => <button aria-label={`查看点位集 ${row.name}`} className="point-id-button" onClick={() => onEdit(row)} type="button">{row.name}</button> },
    { key: 'project', header: '项目', width: '150px', render: (row) => projectNames.get(row.projectId) ?? row.projectId },
    { key: 'protocol', header: '协议', width: '120px', render: (row) => protocolLabel(row.protocol) },
    { key: 'points', header: '点位', width: '90px', render: (row) => `${row.points.length} 个` },
    { key: 'preview', header: '点位预览', render: (row) => row.points.slice(0, 4).map((point) => point.pointId).join(', ') || '-' },
    { key: 'interval', header: '采集周期', width: '130px', render: (row) => intervalSummary(row.points) },
    { key: 'actions', header: '操作', width: '190px', render: (row) => <div className="row-actions"><button aria-label={`修改点位集 ${row.name}`} className="secondary-button compact" onClick={() => onEdit(row)} type="button"><Edit3 aria-hidden="true" size={14} />修改</button><button aria-label={`删除点位集 ${row.name}`} className="danger-button compact" onClick={() => onDelete(row)} type="button"><Trash2 aria-hidden="true" size={14} />删除</button></div> },
  ];
}

function emptyPointSet(projectId: string): PointSetEditorState {
  return {
    description: '',
    name: '',
    pointSetId: '',
    points: [emptyPoint()],
    projectId,
    protocol: 'ModbusRtu',
  };
}

function emptyPoint(): PointSetPointResponse {
  return {
    address: { kind: 'holding_register', value: '' },
    intervalMs: 1000,
    pointId: '',
    semanticId: '',
    unit: null,
    valueType: 'float32',
  };
}

function changePointSetProtocol(pointSet: PointSetEditorState, protocol: string): PointSetEditorState {
  const address = defaultAddressForProtocol(protocol);
  return {
    ...pointSet,
    protocol,
    points: pointSet.points.map((point) => ({ ...point, address: { ...address } })),
  };
}

function defaultAddressForProtocol(protocol: string): PointSetPointResponse['address'] {
  if (protocol === 'CustomSerial') {
    return { kind: 'custom_serial_frame', value: serializeCustomSerialFrame(defaultCustomSerialFrame()) };
  }
  if (protocol === 'Dlt645') return { kind: 'dlt645_address', value: '' };
  if (protocol === 'Iec101') return { kind: 'iec101_ioa', value: '' };
  if (protocol === 'Simulated') return { kind: 'simulated', value: '' };
  return { kind: 'holding_register', value: '' };
}

function defaultCustomSerialFrame(): CustomSerialFrameSpec {
  return {
    offset: 0,
    requestChecksum: 'none',
    requestHex: '',
    responseChecksum: 'none',
    responsePrefixHex: '',
    scale: 1,
    valueEncoding: 'u16_be',
    valueOffset: 0,
  };
}

function parseCustomSerialFrame(value: string): CustomSerialFrameSpec {
  try {
    return { ...defaultCustomSerialFrame(), ...JSON.parse(value) } as CustomSerialFrameSpec;
  } catch {
    return defaultCustomSerialFrame();
  }
}

function serializeCustomSerialFrame(frame: CustomSerialFrameSpec): string {
  const value: Record<string, unknown> = {
    requestHex: frame.requestHex,
    requestChecksum: frame.requestChecksum,
    responseChecksum: frame.responseChecksum,
    valueOffset: frame.valueOffset,
    valueEncoding: frame.valueEncoding,
    scale: frame.scale,
    offset: frame.offset,
  };
  if (frame.responsePrefixHex.trim()) value.responsePrefixHex = frame.responsePrefixHex.trim();
  if (frame.valueEncoding === 'utf8') value.valueLength = frame.valueLength;
  return JSON.stringify(value);
}

function validatePointSet(pointSet: PointSetEditorState): string | undefined {
  if (!pointSet.pointSetId.trim()) return '请填写点位集 ID';
  if (!/^[A-Za-z0-9_.-]+$/.test(pointSet.pointSetId)) return '点位集 ID 只能包含字母、数字、点、下划线和横线';
  if (!pointSet.name.trim()) return '请填写点位集名称';
  if (!pointSet.projectId) return '请选择所属项目';
  if (pointSet.points.length === 0) return '请至少添加一个点位';
  const ids = new Set<string>();
  for (const [index, point] of pointSet.points.entries()) {
    if (!point.pointId.trim() || !point.semanticId.trim() || !point.address.value.trim()) return `请补全第 ${index + 1} 个点位`;
    if (pointSet.protocol === 'CustomSerial') {
      const frame = parseCustomSerialFrame(point.address.value);
      if (!frame.requestHex.trim()) return `请填写第 ${index + 1} 个点位的请求帧`;
      if (!/^(?:0x)?[0-9a-fA-F\s:-]+$/.test(frame.requestHex) || frame.requestHex.replace(/^(?:0x)/i, '').replace(/[\s:-]/g, '').length % 2 !== 0) return `第 ${index + 1} 个点位的请求帧 HEX 格式不正确`;
      if (frame.valueEncoding === 'utf8' && (!frame.valueLength || frame.valueLength < 1)) return `第 ${index + 1} 个点位的文本长度必须大于 0`;
    }
    if (ids.has(point.pointId)) return `点位 ID ${point.pointId} 重复`;
    if (point.intervalMs < 1) return `第 ${index + 1} 个点位采集周期必须大于 0`;
    ids.add(point.pointId);
  }
  return undefined;
}

function protocolLabel(protocol: string): string {
  return protocolOptions.find(([value]) => value === protocol)?.[1] ?? protocol;
}

function intervalSummary(points: PointSetPointResponse[]): string {
  const intervals = Array.from(new Set(points.map((point) => point.intervalMs)));
  if (intervals.length === 0) return '-';
  if (intervals.length === 1) return `${intervals[0]}ms`;
  return `${Math.min(...intervals)}-${Math.max(...intervals)}ms`;
}
