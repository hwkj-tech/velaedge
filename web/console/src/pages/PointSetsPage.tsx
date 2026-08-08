import { useEffect, useMemo, useState } from 'react';
import { Edit3, Plus, Trash2, X } from 'lucide-react';

import type {
  BacnetIpCatalogResponse,
  Dlt645DataIdentifierTemplateResponse,
  PointSetPointResponse,
  PointSetResponse,
  SavePointSetRequest,
  RuntimeProtocolDescriptor,
} from '../api/types';
import { fetchBacnetIpCatalog } from '../api/client';
import { DataTable, type DataTableColumn } from '../components/DataTable';
import { Modal } from '../components/Modal';
import { displayError } from '../utils/errors';
import { protocolOptionsFromCatalog } from '../protocolCatalog';
import './PointMappingsPage.css';

interface ProjectOption {
  projectId: string;
  name: string;
}

interface PointSetEditorState extends SavePointSetRequest {}

interface CustomSerialFrameSpec {
  frameEncoding: string;
  offset: number;
  requestChecksum: string;
  requestHex: string;
  responseChecksum: string;
  responsePrefixHex: string;
  scale: number;
  schemaVersion: number;
  valueEncoding: string;
  valueLength?: number;
  valueOffset: number;
}

interface OpcUaBrowsePathSpec {
  startingNode: string;
  elements: Array<{
    namespaceIndex: number;
    targetName: string;
  }>;
}

type SiemensS7Area = 'DB' | 'M' | 'I' | 'Q';
type SiemensS7DataType = 'bit' | 'byte' | 'word' | 'dword' | 'int' | 'dint' | 'real';

interface SiemensS7AddressSpec {
  area: SiemensS7Area;
  bitOffset: number;
  byteOffset: number;
  dataType: SiemensS7DataType;
  dbNumber: number;
}

type OmronFinsArea = 'CIO' | 'W' | 'H' | 'D' | 'A';

interface OmronFinsAddressSpec {
  area: OmronFinsArea;
  bit?: number;
  word: number;
}

export function PointSetsPage({
  dlt645DataIdentifiers,
  onCreate,
  onDelete,
  onSave,
  pointSets,
  protocolCatalog,
  projects,
}: {
  dlt645DataIdentifiers: Dlt645DataIdentifierTemplateResponse[];
  onCreate: (request: SavePointSetRequest) => Promise<PointSetResponse>;
  onDelete: (pointSetId: string) => Promise<void>;
  onSave: (
    pointSetId: string,
    request: SavePointSetRequest,
  ) => Promise<PointSetResponse>;
  pointSets: PointSetResponse[];
  protocolCatalog?: RuntimeProtocolDescriptor[];
  projects: ProjectOption[];
}) {
  const protocolOptions = protocolOptionsFromCatalog(protocolCatalog);
  const [editor, setEditor] = useState<PointSetEditorState>();
  const [editingId, setEditingId] = useState<string>();
  const [deleteTarget, setDeleteTarget] = useState<PointSetResponse>();
  const [message, setMessage] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [bacnetCatalog, setBacnetCatalog] = useState<BacnetIpCatalogResponse>({
    objectTypes: [],
    properties: [],
  });

  useEffect(() => {
    if (editor?.protocol !== 'BacnetIp' || bacnetCatalog.objectTypes.length > 0) return;
    let cancelled = false;
    void fetchBacnetIpCatalog()
      .then((catalog) => {
        if (!cancelled) setBacnetCatalog(catalog);
      })
      .catch((error) => {
        if (!cancelled) setMessage(`加载 BACnet/IP 对象目录失败：${displayError(error)}`);
      });
    return () => {
      cancelled = true;
    };
  }, [editor?.protocol, bacnetCatalog.objectTypes.length]);

  const openCreate = () => {
    setEditingId(undefined);
    setMessage('');
    setEditor(emptyPointSet(projects[0]?.projectId ?? ''));
  };

  const openEdit = (pointSet: PointSetResponse) => {
    setEditingId(pointSet.pointSetId);
    setEditor({
      description: pointSet.description,
      name: pointSet.name,
      pointSetId: pointSet.pointSetId,
      points: pointSet.points.map((point) => ({
        ...point,
        address: {
          ...point.address,
          modbus: point.address.modbus ? { ...point.address.modbus } : undefined,
        },
      })),
      projectId: pointSet.projectId,
      protocol: pointSet.protocol,
    });
  };

  const submit = async () => {
    if (!editor) return;
    const error = validatePointSet(editor, dlt645DataIdentifiers);
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
  const columns = pointSetColumns(projectNames, protocolOptions, openEdit, setDeleteTarget);

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
                  <small>每个点位可独立设置地址、类型、读写权限、单位和采集周期。</small>
                </div>
                <button
                  className="secondary-button compact"
                  onClick={() => setEditor({ ...editor, points: [...editor.points, emptyPointForProtocol(editor.protocol)] })}
                  type="button"
                >
                  <Plus aria-hidden="true" size={14} />
                  添加点位
                </button>
              </div>
              {editor.protocol !== 'CustomSerial' ? (
                <div className="point-set-row point-set-row-header" aria-hidden="true">
                  <span>Point ID</span><span>语义 ID</span><span>地址类型</span><span>地址值</span>
                  <span>数据类型</span><span>读写权限</span><span>周期(ms)</span><span>单位</span><span />
                </div>
              ) : null}
              <div className="point-set-rows">
                {editor.points.map((point, index) => (
                  <PointRow
                    bacnetCatalog={bacnetCatalog}
                    dlt645DataIdentifiers={dlt645DataIdentifiers}
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

            {message ? (
              <div className="form-validation-panel" role="alert">
                <strong>无法保存点位集</strong>
                <span>{message}</span>
              </div>
            ) : null}
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
  bacnetCatalog,
  dlt645DataIdentifiers,
  index,
  onChange,
  onRemove,
  point,
  protocol,
  removable,
}: {
  bacnetCatalog: BacnetIpCatalogResponse;
  dlt645DataIdentifiers: Dlt645DataIdentifierTemplateResponse[];
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
  const showModbusOptions = isModbusProtocol(protocol) && isModbusRegisterAddress(point.address.kind);
  const showDlt645Options = protocol === 'Dlt645';
  const showBacnetOptions = protocol === 'BacnetIp';
  const showSiemensS7Options = protocol === 'SiemensS7';
  const showOmronFinsOptions = protocol === 'OmronFins';
  const showOpcUaBrowsePath = protocol === 'OpcUa' && point.address.kind === 'browse_path';
  const showOpcUaWriteOptions = protocol === 'OpcUa' && point.access !== 'read_only';
  const showIec101ControlOptions = protocol === 'Iec101' && point.access !== 'read_only';
  const showIec104ControlOptions = protocol === 'Iec104' && point.access !== 'read_only';
  const siemensS7Address = parseSiemensS7Address(point.address.value, point.valueType);
  const siemensS7ReadOnly = showSiemensS7Options && siemensS7Address.area === 'I';
  const bacnetAddress = parseBacnetAddressValue(point.address.value);
  const bacnetWritable = bacnetCatalog.objectTypes.find(
    (objectType) => objectType.objectType === bacnetAddress.objectType,
  )?.writable ?? false;
  const modbus = point.address.modbus ?? defaultModbusOptions(point.valueType);
  const updateModbus = (patch: Partial<NonNullable<PointSetPointResponse['address']['modbus']>>) => {
    onChange({
      ...point,
      access: 'bitIndex' in patch && patch.bitIndex !== undefined ? 'read_only' : point.access,
      address: {
        ...point.address,
        modbus: { ...modbus, ...patch },
      },
    });
  };
  return (
    <section className={showModbusOptions ? 'modbus-point-row' : showDlt645Options || showBacnetOptions || showSiemensS7Options || showOmronFinsOptions || showOpcUaBrowsePath || showOpcUaWriteOptions || showIec101ControlOptions || showIec104ControlOptions ? 'dlt645-point-row' : undefined}>
      <div className="point-set-row">
        <input aria-label={`点位 ${index + 1} Point ID`} value={point.pointId} onChange={(event) => onChange({ ...point, pointId: event.target.value })} />
        <input aria-label={`点位 ${index + 1} 语义 ID`} value={point.semanticId} onChange={(event) => onChange({ ...point, semanticId: event.target.value })} />
        <select aria-label={`点位 ${index + 1} 地址类型`} value={point.address.kind} onChange={(event) => {
          const kind = event.target.value;
          onChange({
            ...point,
            access: isProtocolReadOnlyAddress(kind) ? 'read_only' : point.access,
            address: {
              ...point.address,
              kind,
              value: kind === 'browse_path'
                ? serializeOpcUaBrowsePath(defaultOpcUaBrowsePath())
                : kind === 'node_id'
                  ? ''
                  : point.address.value,
              modbus: isModbusProtocol(protocol) && isModbusRegisterAddress(kind)
                ? (point.address.modbus ?? defaultModbusOptions(point.valueType))
                : undefined,
            },
          });
        }}>
          {addressKindOptions(protocol).map(([value, label]) => (
            <option key={value} value={value}>{label}</option>
          ))}
        </select>
        <input
          aria-label={`点位 ${index + 1} 地址值`}
          placeholder={addressValuePlaceholder(protocol)}
          readOnly={showBacnetOptions || showSiemensS7Options || showOmronFinsOptions || showOpcUaBrowsePath}
          value={showOpcUaBrowsePath ? opcUaBrowsePathSummary(point.address.value) : point.address.value}
          onChange={(event) => onChange({ ...point, address: { ...point.address, value: event.target.value } })}
        />
        <select aria-label={`点位 ${index + 1} 数据类型`} value={point.valueType} onChange={(event) => {
          const valueType = event.target.value;
          onChange({
            ...point,
            valueType,
            opcUa: protocol === 'OpcUa'
              ? { writeDataType: defaultOpcUaWriteDataType(valueType) }
              : point.opcUa,
            iec101: protocol === 'Iec101' && point.access !== 'read_only'
              ? defaultIec101PointOptions(valueType, point.iec101?.selectBeforeOperate)
              : point.iec101,
            iec104: protocol === 'Iec104' && point.access !== 'read_only'
              ? defaultIec104PointOptions(valueType, point.iec104?.selectBeforeOperate)
              : point.iec104,
            address: showModbusOptions
              ? { ...point.address, modbus: defaultModbusOptions(valueType) }
              : showSiemensS7Options
                ? {
                    ...point.address,
                    value: serializeSiemensS7Address(
                      normalizeSiemensS7ForValueType(siemensS7Address, valueType),
                    ),
                  }
                : showOmronFinsOptions
                  ? {
                      ...point.address,
                      value: serializeOmronFinsAddress(
                        normalizeOmronFinsForValueType(
                          parseOmronFinsAddress(point.address.value, point.valueType),
                          valueType,
                        ),
                      ),
                    }
                  : point.address,
          });
        }}>
          <option value="float32">float32</option><option value="float64">float64</option>
          <option value="int32">int32</option><option value="int64">int64</option>
          <option value="bool">bool</option><option disabled={showSiemensS7Options || showOmronFinsOptions} value="string">string</option>
        </select>
        <select aria-label={`点位 ${index + 1} 读写权限`} value={point.access} onChange={(event) => {
          const access = event.target.value as PointSetPointResponse['access'];
          onChange({
            ...point,
            access,
            opcUa: protocol === 'OpcUa' && access !== 'read_only'
              ? (point.opcUa ?? { writeDataType: defaultOpcUaWriteDataType(point.valueType) })
              : point.opcUa,
            iec101: protocol === 'Iec101' && access !== 'read_only'
              ? (point.iec101 ?? defaultIec101PointOptions(point.valueType))
              : point.iec101,
            iec104: protocol === 'Iec104' && access !== 'read_only'
              ? (point.iec104 ?? defaultIec104PointOptions(point.valueType))
              : point.iec104,
          });
        }}>
          <option value="read_only">只读</option>
          <option disabled={isProtocolReadOnlyAddress(point.address.kind) || siemensS7ReadOnly || modbus.bitIndex !== undefined || (showBacnetOptions && !bacnetWritable) || ((protocol === 'Iec101' || protocol === 'Iec104') && point.valueType === 'string')} value="read_write">读写</option>
          <option disabled={isProtocolReadOnlyAddress(point.address.kind) || siemensS7ReadOnly || modbus.bitIndex !== undefined || (showBacnetOptions && !bacnetWritable) || ((protocol === 'Iec101' || protocol === 'Iec104') && point.valueType === 'string')} value="write_only">只写</option>
        </select>
        <input aria-label={`点位 ${index + 1} 采集周期(ms)`} min="100" step="100" type="number" value={point.intervalMs} onChange={(event) => onChange({ ...point, intervalMs: Number(event.target.value) })} />
        <input aria-label={`点位 ${index + 1} 单位`} value={point.unit ?? ''} onChange={(event) => onChange({ ...point, unit: event.target.value || null })} />
        <button aria-label={`移除点位 ${index + 1}`} className="danger-button compact" disabled={!removable} onClick={onRemove} type="button"><Trash2 aria-hidden="true" size={14} /></button>
      </div>
      {showModbusOptions ? (
        <div aria-label={`点位 ${index + 1} Modbus 解码`} className="modbus-point-options">
          {point.valueType !== 'bool' && point.valueType !== 'string' ? (
            <label>
              <span>寄存器编码</span>
              <select aria-label={`点位 ${index + 1} 寄存器编码`} value={modbus.encoding ?? ''} onChange={(event) => updateModbus({ encoding: event.target.value ? event.target.value as NonNullable<typeof modbus.encoding> : undefined })}>
                {modbusEncodingOptions(point.valueType).map(([value, label]) => <option key={value || 'auto'} value={value}>{label}</option>)}
              </select>
            </label>
          ) : null}
          <label>
            <span>字节序</span>
            <select aria-label={`点位 ${index + 1} 字节序`} value={modbus.byteOrder} onChange={(event) => updateModbus({ byteOrder: event.target.value as typeof modbus.byteOrder })}>
              <option value="big_endian">大端 AB</option>
              <option value="little_endian">小端 BA</option>
            </select>
          </label>
          {point.valueType !== 'bool' && point.valueType !== 'string' ? (
            <label>
              <span>字序</span>
              <select aria-label={`点位 ${index + 1} 字序`} value={modbus.wordOrder} onChange={(event) => updateModbus({ wordOrder: event.target.value as typeof modbus.wordOrder })}>
                <option value="high_word_first">高字在前 ABCD</option>
                <option value="low_word_first">低字在前 CDAB</option>
              </select>
            </label>
          ) : null}
          {point.valueType !== 'bool' && point.valueType !== 'string' ? (
            <>
              <label><span>缩放系数</span><input aria-label={`点位 ${index + 1} 缩放系数`} step="any" type="number" value={modbus.scale} onChange={(event) => updateModbus({ scale: Number(event.target.value) })} /></label>
              <label><span>偏移量</span><input aria-label={`点位 ${index + 1} 偏移量`} step="any" type="number" value={modbus.offset} onChange={(event) => updateModbus({ offset: Number(event.target.value) })} /></label>
            </>
          ) : null}
          {point.valueType === 'bool' ? (
            <label>
              <span>寄存器位（可选）</span>
              <input aria-label={`点位 ${index + 1} 寄存器位`} max="15" min="0" placeholder="0-15" type="number" value={modbus.bitIndex ?? ''} onChange={(event) => updateModbus({ bitIndex: event.target.value === '' ? undefined : Number(event.target.value) })} />
            </label>
          ) : null}
        </div>
      ) : null}
      {showDlt645Options ? (
        <Dlt645PointOptions
          index={index}
          onChange={onChange}
          point={point}
          templates={dlt645DataIdentifiers}
        />
      ) : null}
      {showSiemensS7Options ? (
        <SiemensS7PointOptions
          index={index}
          onChange={onChange}
          point={point}
        />
      ) : null}
      {showOmronFinsOptions ? (
        <OmronFinsPointOptions
          index={index}
          onChange={onChange}
          point={point}
        />
      ) : null}
      {showOpcUaBrowsePath ? (
        <OpcUaBrowsePathOptions
          index={index}
          onChange={onChange}
          point={point}
        />
      ) : null}
      {showOpcUaWriteOptions ? (
        <div aria-label={`点位 ${index + 1} OPC UA 写入`} className="modbus-point-options">
          <label>
            <span>UA 写入类型</span>
            <select
              aria-label={`点位 ${index + 1} OPC UA 写入类型`}
              value={point.opcUa?.writeDataType ?? defaultOpcUaWriteDataType(point.valueType)}
              onChange={(event) => onChange({
                ...point,
                opcUa: {
                  writeDataType: event.target.value as NonNullable<PointSetPointResponse['opcUa']>['writeDataType'],
                },
              })}
            >
              {opcUaWriteDataTypeOptions(point.valueType).map(([value, label]) => (
                <option key={value} value={value}>{label}</option>
              ))}
            </select>
          </label>
        </div>
      ) : null}
      {showIec101ControlOptions ? (
        <div aria-label={`点位 ${index + 1} IEC 101 控制`} className="modbus-point-options">
          <label>
            <span>控制类型</span>
            <select
              aria-label={`点位 ${index + 1} IEC 101 控制类型`}
              value={point.iec101?.controlType ?? defaultIec101ControlType(point.valueType) ?? ''}
              onChange={(event) => onChange({
                ...point,
                iec101: {
                  controlType: event.target.value as NonNullable<PointSetPointResponse['iec101']>['controlType'],
                  selectBeforeOperate: point.iec101?.selectBeforeOperate ?? false,
                },
              })}
            >
              {iec101ControlTypeOptions(point.valueType).map(([value, label]) => (
                <option key={value} value={value}>{label}</option>
              ))}
            </select>
          </label>
          <label className="checkbox-field">
            <input
              aria-label={`点位 ${index + 1} IEC 101 选择后执行`}
              checked={point.iec101?.selectBeforeOperate ?? false}
              onChange={(event) => {
                const controlType = point.iec101?.controlType ?? defaultIec101ControlType(point.valueType);
                if (!controlType) return;
                onChange({
                  ...point,
                  iec101: { controlType, selectBeforeOperate: event.target.checked },
                });
              }}
              type="checkbox"
            />
            <span>选择后执行（SBO）</span>
          </label>
        </div>
      ) : null}
      {showIec104ControlOptions ? (
        <div aria-label={`点位 ${index + 1} IEC 104 控制`} className="modbus-point-options">
          <label>
            <span>控制类型</span>
            <select
              aria-label={`点位 ${index + 1} IEC 104 控制类型`}
              value={point.iec104?.controlType ?? defaultIec104ControlType(point.valueType) ?? ''}
              onChange={(event) => onChange({
                ...point,
                iec104: {
                  controlType: event.target.value as NonNullable<PointSetPointResponse['iec104']>['controlType'],
                  selectBeforeOperate: point.iec104?.selectBeforeOperate ?? false,
                },
              })}
            >
              {iec104ControlTypeOptions(point.valueType).map(([value, label]) => (
                <option key={value} value={value}>{label}</option>
              ))}
            </select>
          </label>
          <label className="checkbox-field">
            <input
              aria-label={`点位 ${index + 1} IEC 104 选择后执行`}
              checked={point.iec104?.selectBeforeOperate ?? false}
              onChange={(event) => {
                const controlType = point.iec104?.controlType ?? defaultIec104ControlType(point.valueType);
                if (!controlType) return;
                onChange({
                  ...point,
                  iec104: { controlType, selectBeforeOperate: event.target.checked },
                });
              }}
              type="checkbox"
            />
            <span>选择后执行（SBO）</span>
          </label>
        </div>
      ) : null}
      {showBacnetOptions ? (
        <BacnetPointOptions
          catalog={bacnetCatalog}
          index={index}
          onChange={onChange}
          point={point}
        />
      ) : null}
    </section>
  );
}

function SiemensS7PointOptions({
  index,
  onChange,
  point,
}: {
  index: number;
  onChange: (point: PointSetPointResponse) => void;
  point: PointSetPointResponse;
}) {
  const address = parseSiemensS7Address(point.address.value, point.valueType);
  const updateAddress = (next: SiemensS7AddressSpec) => onChange({
    ...point,
    access: next.area === 'I' ? 'read_only' : point.access,
    address: { ...point.address, value: serializeSiemensS7Address(next) },
  });
  const typeOptions = address.area === 'DB'
    ? siemensS7DbDataTypeOptions
    : siemensS7MemoryDataTypeOptions;

  return (
    <div aria-label={`点位 ${index + 1} Siemens S7 地址`} className="modbus-point-options protocol-point-options">
      <label>
        <span>存储区</span>
        <select
          aria-label={`点位 ${index + 1} S7 存储区`}
          value={address.area}
          onChange={(event) => {
            const area = event.target.value as SiemensS7Area;
            const normalizedType = area === 'DB'
              ? address.dataType
              : normalizeSiemensS7MemoryDataType(address.dataType, point.valueType);
            updateAddress({ ...address, area, dataType: normalizedType });
          }}
        >
          <option value="DB">数据块 DB</option>
          <option value="M">标志区 M</option>
          <option value="I">过程输入 I（只读）</option>
          <option value="Q">过程输出 Q</option>
        </select>
      </label>
      {address.area === 'DB' ? (
        <label>
          <span>DB 编号</span>
          <input
            aria-label={`点位 ${index + 1} S7 DB 编号`}
            max="65535"
            min="0"
            type="number"
            value={address.dbNumber}
            onChange={(event) => updateAddress({ ...address, dbNumber: Number(event.target.value) })}
          />
        </label>
      ) : null}
      <label>
        <span>数据格式</span>
        <select
          aria-label={`点位 ${index + 1} S7 数据格式`}
          value={address.dataType}
          onChange={(event) => {
            const dataType = event.target.value as SiemensS7DataType;
            const valueType = siemensS7ValueType(dataType, point.valueType);
            onChange({
              ...point,
              valueType,
              address: {
                ...point.address,
                value: serializeSiemensS7Address({ ...address, dataType }),
              },
            });
          }}
        >
          {typeOptions.map(([value, label]) => (
            <option key={value} value={value}>{label}</option>
          ))}
        </select>
      </label>
      <label>
        <span>字节偏移</span>
        <input
          aria-label={`点位 ${index + 1} S7 字节偏移`}
          min="0"
          type="number"
          value={address.byteOffset}
          onChange={(event) => updateAddress({ ...address, byteOffset: Number(event.target.value) })}
        />
      </label>
      {address.dataType === 'bit' ? (
        <label>
          <span>位号</span>
          <input
            aria-label={`点位 ${index + 1} S7 位号`}
            max="7"
            min="0"
            type="number"
            value={address.bitOffset}
            onChange={(event) => updateAddress({ ...address, bitOffset: Number(event.target.value) })}
          />
        </label>
      ) : null}
      <div className="protocol-address-preview">
        <span>Runtime 地址</span>
        <code>{serializeSiemensS7Address(address)}</code>
      </div>
    </div>
  );
}

function OmronFinsPointOptions({
  index,
  onChange,
  point,
}: {
  index: number;
  onChange: (point: PointSetPointResponse) => void;
  point: PointSetPointResponse;
}) {
  const address = parseOmronFinsAddress(point.address.value, point.valueType);
  const updateAddress = (next: OmronFinsAddressSpec) => onChange({
    ...point,
    address: { ...point.address, value: serializeOmronFinsAddress(next) },
  });

  return (
    <div aria-label={`点位 ${index + 1} Omron FINS 地址`} className="modbus-point-options protocol-point-options">
      <label>
        <span>存储区</span>
        <select
          aria-label={`点位 ${index + 1} FINS 存储区`}
          value={address.area}
          onChange={(event) => {
            const area = event.target.value as OmronFinsArea;
            updateAddress({
              ...address,
              area,
              bit: area === 'D' ? undefined : address.bit,
            });
          }}
        >
          <option value="CIO">CIO 区</option>
          <option value="W">工作区 W</option>
          <option value="H">保持区 H</option>
          <option value="D" disabled={point.valueType === 'bool'}>数据存储区 D/DM</option>
          <option value="A">辅助区 A</option>
        </select>
      </label>
      <label>
        <span>字地址</span>
        <input
          aria-label={`点位 ${index + 1} FINS 字地址`}
          max={omronFinsWordCapacity(address.area) - 1}
          min="0"
          type="number"
          value={address.word}
          onChange={(event) => updateAddress({ ...address, word: Number(event.target.value) })}
        />
      </label>
      {point.valueType === 'bool' ? (
        <label>
          <span>位号</span>
          <input
            aria-label={`点位 ${index + 1} FINS 位号`}
            max="15"
            min="0"
            type="number"
            value={address.bit ?? 0}
            onChange={(event) => updateAddress({ ...address, bit: Number(event.target.value) })}
          />
        </label>
      ) : null}
      <div className="protocol-address-preview">
        <span>Runtime 地址</span>
        <code>{serializeOmronFinsAddress(address)}</code>
      </div>
    </div>
  );
}

function OpcUaBrowsePathOptions({
  index,
  onChange,
  point,
}: {
  index: number;
  onChange: (point: PointSetPointResponse) => void;
  point: PointSetPointResponse;
}) {
  const path = parseOpcUaBrowsePath(point.address.value);
  const updatePath = (next: OpcUaBrowsePathSpec) => onChange({
    ...point,
    address: { ...point.address, value: serializeOpcUaBrowsePath(next) },
  });
  return (
    <div aria-label={`点位 ${index + 1} OPC UA 语义路径`} className="opc-ua-browse-path-options">
      <label className="opc-ua-starting-node">
        <span>起始 NodeId</span>
        <input
          aria-label={`点位 ${index + 1} 起始 NodeId`}
          placeholder="i=85"
          value={path.startingNode}
          onChange={(event) => updatePath({ ...path, startingNode: event.target.value })}
        />
      </label>
      <div className="opc-ua-path-elements">
        <div className="opc-ua-path-heading">
          <span>QualifiedName 路径</span>
          <button
            className="secondary-button compact"
            onClick={() => updatePath({
              ...path,
              elements: [...path.elements, { namespaceIndex: 2, targetName: '' }],
            })}
            type="button"
          >
            <Plus aria-hidden="true" size={13} />路径段
          </button>
        </div>
        {path.elements.map((element, elementIndex) => (
          <div className="opc-ua-path-element" key={elementIndex}>
            <input
              aria-label={`点位 ${index + 1} 路径段 ${elementIndex + 1} 命名空间`}
              min="0"
              type="number"
              value={element.namespaceIndex}
              onChange={(event) => updatePath({
                ...path,
                elements: path.elements.map((item, itemIndex) => itemIndex === elementIndex
                  ? { ...item, namespaceIndex: Number(event.target.value) }
                  : item),
              })}
            />
            <input
              aria-label={`点位 ${index + 1} 路径段 ${elementIndex + 1} 名称`}
              placeholder="Machine"
              value={element.targetName}
              onChange={(event) => updatePath({
                ...path,
                elements: path.elements.map((item, itemIndex) => itemIndex === elementIndex
                  ? { ...item, targetName: event.target.value }
                  : item),
              })}
            />
            <button
              aria-label={`移除点位 ${index + 1} 路径段 ${elementIndex + 1}`}
              className="icon-button danger-icon"
              disabled={path.elements.length === 1}
              onClick={() => updatePath({
                ...path,
                elements: path.elements.filter((_, itemIndex) => itemIndex !== elementIndex),
              })}
              title="移除路径段"
              type="button"
            >
              <Trash2 aria-hidden="true" size={14} />
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

function BacnetPointOptions({
  catalog,
  index,
  onChange,
  point,
}: {
  catalog: BacnetIpCatalogResponse;
  index: number;
  onChange: (point: PointSetPointResponse) => void;
  point: PointSetPointResponse;
}) {
  const address = parseBacnetAddressValue(point.address.value);
  const object = catalog.objectTypes.find((item) => item.objectType === address.objectType);
  const writable = object?.writable ?? bacnetObjectIsWritable(address.objectType);
  const updateAddress = (patch: Partial<typeof address>) => {
    const next = { ...address, ...patch };
    const nextObject = catalog.objectTypes.find((item) => item.objectType === next.objectType);
    onChange({
      ...point,
      access: nextObject?.writable ? point.access : 'read_only',
      address: {
        kind: 'bacnet_object_property',
        value: serializeBacnetAddressValue(next),
      },
    });
  };
  return (
    <div aria-label={`点位 ${index + 1} BACnet/IP 参数`} className="dlt645-point-options">
      <label>
        <span>设备实例号</span>
        <input
          aria-label={`点位 ${index + 1} BACnet 设备实例号`}
          min="0"
          max="4194302"
          type="number"
          value={address.deviceInstance}
          onChange={(event) => updateAddress({ deviceInstance: event.target.value })}
        />
      </label>
      <label>
        <span>对象类型</span>
        <select
          aria-label={`点位 ${index + 1} BACnet 对象类型`}
          value={address.objectType}
          onChange={(event) => updateAddress({ objectType: event.target.value })}
        >
          {catalog.objectTypes.length === 0 ? (
            <option value={address.objectType}>{address.objectType || '加载中'}</option>
          ) : null}
          {catalog.objectTypes.map((item) => (
            <option key={item.objectType} value={item.objectType}>
              {item.name} · {item.objectType}{item.writable ? ' · 可写' : ''}
            </option>
          ))}
        </select>
      </label>
      <label>
        <span>对象实例号</span>
        <input
          aria-label={`点位 ${index + 1} BACnet 对象实例号`}
          min="0"
          max="4194302"
          type="number"
          value={address.objectInstance}
          onChange={(event) => updateAddress({ objectInstance: event.target.value })}
        />
      </label>
      <label>
        <span>属性</span>
        <select
          aria-label={`点位 ${index + 1} BACnet 属性`}
          value={address.property}
          onChange={(event) => updateAddress({ property: event.target.value })}
        >
          {catalog.properties.length === 0 ? (
            <option value={address.property}>{address.property || '加载中'}</option>
          ) : null}
          {catalog.properties.map((item) => (
            <option key={item.property} value={item.property}>
              {item.name} · {item.property}
            </option>
          ))}
        </select>
      </label>
      <label>
        <span>数组索引（可选）</span>
        <input
          aria-label={`点位 ${index + 1} BACnet 数组索引`}
          min="0"
          type="number"
          value={address.arrayIndex}
          onChange={(event) => updateAddress({ arrayIndex: event.target.value })}
        />
      </label>
      <label>
        <span>写入优先级</span>
        <input
          aria-label={`点位 ${index + 1} BACnet 写入优先级`}
          disabled={!writable}
          max="16"
          min="1"
          title="BACnet 命令优先级，1 最高，16 最低"
          type="number"
          value={point.bacnet?.writePriority ?? 16}
          onChange={(event) => onChange({
            ...point,
            bacnet: { writePriority: Number(event.target.value) },
          })}
        />
      </label>
    </div>
  );
}

function Dlt645PointOptions({
  index,
  onChange,
  point,
  templates,
}: {
  index: number;
  onChange: (point: PointSetPointResponse) => void;
  point: PointSetPointResponse;
  templates: Dlt645DataIdentifierTemplateResponse[];
}) {
  const address = parseDlt645AddressValue(point.address.value);
  const selected = templates.find(
    (template) => template.dataIdentifier.toUpperCase() === address.dataIdentifier.toUpperCase(),
  );
  const updateAddress = (patch: Partial<typeof address>) => {
    const next = { ...address, ...patch };
    onChange({
      ...point,
      access: 'read_only',
      address: {
        kind: 'dlt645_address',
        value: serializeDlt645AddressValue(next),
      },
    });
  };
  const applyTemplate = (templateId: string) => {
    const template = templates.find((item) => item.templateId === templateId);
    if (!template) {
      updateAddress({ dataIdentifier: '', valueBytes: '' });
      return;
    }
    onChange({
      ...point,
      access: 'read_only',
      pointId: point.pointId || template.templateId,
      semanticId: point.semanticId || template.semanticId,
      valueType: pointSetEditorValueType(template.valueType),
      unit: template.unit ?? null,
      address: {
        kind: 'dlt645_address',
        value: serializeDlt645AddressValue({
          dataIdentifier: template.dataIdentifier,
          decimalPlaces: String(template.decimalPlaces),
          meterAddress: address.meterAddress,
          valueBytes: '',
        }),
      },
    });
  };

  return (
    <div aria-label={`点位 ${index + 1} DL/T 645 参数`} className="dlt645-point-options">
      <label className="dlt645-template-field">
        <span>常用数据标识</span>
        <select
          aria-label={`点位 ${index + 1} 常用数据标识`}
          onChange={(event) => applyTemplate(event.target.value)}
          value={selected?.templateId ?? ''}
        >
          <option value="">自定义数据标识</option>
          {templates.map((template) => (
            <option key={template.templateId} value={template.templateId}>
              {template.name} · {template.dataIdentifier}
            </option>
          ))}
        </select>
      </label>
      <label>
        <span>电表通信地址</span>
        <input
          aria-label={`点位 ${index + 1} 电表通信地址`}
          inputMode="numeric"
          maxLength={12}
          placeholder="12 位，例如 123456789012"
          value={address.meterAddress}
          onChange={(event) => updateAddress({ meterAddress: event.target.value.replace(/\D/g, '').slice(0, 12) })}
        />
      </label>
      <label>
        <span>数据标识 DI</span>
        <input
          aria-label={`点位 ${index + 1} 数据标识 DI`}
          maxLength={8}
          placeholder="8 位十六进制"
          value={address.dataIdentifier}
          onChange={(event) => updateAddress({ dataIdentifier: event.target.value.replace(/[^0-9a-f]/gi, '').slice(0, 8).toUpperCase() })}
        />
      </label>
      <label>
        <span>小数位</span>
        <input
          aria-label={`点位 ${index + 1} 小数位`}
          max="18"
          min="0"
          type="number"
          value={address.decimalPlaces}
          onChange={(event) => updateAddress({ decimalPlaces: event.target.value })}
        />
      </label>
      <label>
        <span>响应值字节数</span>
        <input
          aria-label={`点位 ${index + 1} 响应值字节数`}
          max="251"
          min="1"
          placeholder="按厂商手册填写"
          readOnly={Boolean(selected)}
          type="number"
          value={selected ? String(selected.valueBytes) : address.valueBytes}
          onChange={(event) => updateAddress({ valueBytes: event.target.value })}
        />
      </label>
      <div className="dlt645-template-meta">
        <span>长度来源</span>
        <strong>{selected ? '标准目录固定' : '厂商手册'}</strong>
      </div>
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
        <label><span>DSL 版本</span><select aria-label={`点位 ${index + 1} DSL 版本`} value={frame.schemaVersion} onChange={(event) => { const schemaVersion = Number(event.target.value); updateFrame({ schemaVersion, frameEncoding: schemaVersion === 1 ? 'raw' : frame.frameEncoding }); }}><option value="1">v1 兼容</option><option value="2">v2</option></select></label>
        <label><span>成帧方式</span><select aria-label={`点位 ${index + 1} 成帧方式`} disabled={frame.schemaVersion === 1} value={frame.frameEncoding} onChange={(event) => updateFrame({ frameEncoding: event.target.value })}><option value="raw">Raw 原始帧</option><option value="slip">SLIP</option><option value="cobs">COBS</option></select></label>
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
  return <select aria-label={ariaLabel} onChange={(event) => onChange(event.target.value)} value={value}><option value="none">无</option><option value="sum8">SUM8</option><option value="xor8">XOR8</option><option value="modbus_crc16">Modbus CRC16</option><option value="crc16_ccitt_false">CRC-16/CCITT-FALSE</option></select>;
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
  protocolOptions: Array<readonly [string, string]>,
  onEdit: (pointSet: PointSetResponse) => void,
  onDelete: (pointSet: PointSetResponse) => void,
): Array<DataTableColumn<PointSetResponse>> {
  return [
    { key: 'name', header: '点位集', width: '220px', render: (row) => <button aria-label={`查看点位集 ${row.name}`} className="point-id-button" onClick={() => onEdit(row)} type="button">{row.name}</button> },
    { key: 'project', header: '项目', width: '150px', render: (row) => projectNames.get(row.projectId) ?? row.projectId },
    { key: 'protocol', header: '协议', width: '120px', render: (row) => protocolLabel(row.protocol, protocolOptions) },
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
    access: 'read_only',
    address: { kind: 'holding_register', value: '', modbus: defaultModbusOptions('float32') },
    intervalMs: 1000,
    pointId: '',
    semanticId: '',
    unit: null,
    valueType: 'float32',
  };
}

function emptyPointForProtocol(protocol: string): PointSetPointResponse {
  const point = emptyPoint();
  return {
    ...point,
    address: defaultAddressForProtocol(protocol),
    opcUa: protocol === 'OpcUa' ? { writeDataType: defaultOpcUaWriteDataType(point.valueType) } : undefined,
    iec101: undefined,
    iec104: undefined,
    bacnet: protocol === 'BacnetIp' ? { writePriority: 16 } : undefined,
  };
}

function changePointSetProtocol(pointSet: PointSetEditorState, protocol: string): PointSetEditorState {
  const address = defaultAddressForProtocol(protocol);
  return {
    ...pointSet,
    protocol,
    points: pointSet.points.map((point) => ({
      ...point,
      access: 'read_only',
      address: { ...address },
      opcUa: protocol === 'OpcUa' ? { writeDataType: defaultOpcUaWriteDataType(point.valueType) } : undefined,
      iec101: undefined,
      iec104: undefined,
      bacnet: protocol === 'BacnetIp' ? { writePriority: 16 } : undefined,
    })),
  };
}

function isProtocolReadOnlyAddress(kind: string): boolean {
  return kind === 'input_register' || kind === 'discrete_input' || kind === 'dlt645_address';
}

const siemensS7DbDataTypeOptions: Array<[SiemensS7DataType, string]> = [
  ['bit', '位 DBX'],
  ['byte', '无符号字节 DBB'],
  ['word', '无符号字 DBW'],
  ['dword', '无符号双字 DBD'],
  ['int', '有符号整数 INT'],
  ['dint', '有符号双整数 DINT'],
  ['real', '浮点数 REAL'],
];

const siemensS7MemoryDataTypeOptions: Array<[SiemensS7DataType, string]> = [
  ['bit', '位'],
  ['byte', '字节 B'],
  ['word', '字 W'],
  ['dword', '双字 D'],
];

function defaultSiemensS7Address(valueType: string): SiemensS7AddressSpec {
  return {
    area: 'DB',
    bitOffset: 0,
    byteOffset: 0,
    dataType: valueType === 'bool' ? 'bit' : valueType.startsWith('int') ? 'dint' : 'real',
    dbNumber: 1,
  };
}

function tryParseSiemensS7Address(value: string): SiemensS7AddressSpec | undefined {
  const normalized = value.trim().toUpperCase();
  const dbMatch = normalized.match(/^DB(\d+)\.(DBX|DBB|DBW|DBD|DINT|INT|REAL)(\d+)(?:\.(\d+))?$/);
  if (dbMatch) {
    const dataTypeByToken: Record<string, SiemensS7DataType> = {
      DBX: 'bit', DBB: 'byte', DBW: 'word', DBD: 'dword', INT: 'int', DINT: 'dint', REAL: 'real',
    };
    const dataType = dataTypeByToken[dbMatch[2]];
    const bitOffset = Number(dbMatch[4] ?? 0);
    if (!dataType || (dataType === 'bit' ? bitOffset > 7 || dbMatch[4] === undefined : dbMatch[4] !== undefined)) return undefined;
    const dbNumber = Number(dbMatch[1]);
    const byteOffset = Number(dbMatch[3]);
    if (dbNumber > 65_535 || !Number.isSafeInteger(byteOffset)) return undefined;
    return { area: 'DB', bitOffset, byteOffset, dataType, dbNumber };
  }

  const memoryMatch = normalized.match(/^([MIQ])(?:(X|B|W|D))?(\d+)(?:\.(\d+))?$/);
  if (!memoryMatch) return undefined;
  const dataTypeByToken: Record<string, SiemensS7DataType> = {
    X: 'bit', B: 'byte', W: 'word', D: 'dword', '': 'bit',
  };
  const dataType = dataTypeByToken[memoryMatch[2] ?? ''];
  const bitOffset = Number(memoryMatch[4] ?? 0);
  if (dataType === 'bit' ? bitOffset > 7 || memoryMatch[4] === undefined : memoryMatch[4] !== undefined) return undefined;
  const byteOffset = Number(memoryMatch[3]);
  if (!Number.isSafeInteger(byteOffset)) return undefined;
  return {
    area: memoryMatch[1] as SiemensS7Area,
    bitOffset,
    byteOffset,
    dataType,
    dbNumber: 0,
  };
}

function parseSiemensS7Address(value: string, valueType: string): SiemensS7AddressSpec {
  return tryParseSiemensS7Address(value) ?? defaultSiemensS7Address(valueType);
}

function serializeSiemensS7Address(address: SiemensS7AddressSpec): string {
  const byteOffset = Math.max(0, Math.trunc(address.byteOffset || 0));
  const bitOffset = Math.min(7, Math.max(0, Math.trunc(address.bitOffset || 0)));
  if (address.area === 'DB') {
    const dbNumber = Math.min(65_535, Math.max(0, Math.trunc(address.dbNumber || 0)));
    const tokenByType: Record<SiemensS7DataType, string> = {
      bit: 'DBX', byte: 'DBB', word: 'DBW', dword: 'DBD', int: 'INT', dint: 'DINT', real: 'REAL',
    };
    const offset = address.dataType === 'bit' ? `${byteOffset}.${bitOffset}` : String(byteOffset);
    return `DB${dbNumber}.${tokenByType[address.dataType]}${offset}`;
  }
  if (address.dataType === 'bit') return `${address.area}${byteOffset}.${bitOffset}`;
  const suffix = address.dataType === 'byte' ? 'B' : address.dataType === 'word' ? 'W' : 'D';
  return `${address.area}${suffix}${byteOffset}`;
}

function normalizeSiemensS7MemoryDataType(dataType: SiemensS7DataType, valueType: string): SiemensS7DataType {
  if (valueType === 'bool') return 'bit';
  if (valueType.startsWith('float')) return 'dword';
  if (dataType === 'int') return 'word';
  if (dataType === 'dint' || dataType === 'real') return 'dword';
  return dataType === 'bit' ? 'dword' : dataType;
}

function normalizeSiemensS7ForValueType(address: SiemensS7AddressSpec, valueType: string): SiemensS7AddressSpec {
  if (valueType === 'bool') return { ...address, dataType: 'bit' };
  if (valueType.startsWith('float')) return { ...address, dataType: address.area === 'DB' ? 'real' : 'dword' };
  if (valueType.startsWith('int')) return { ...address, dataType: address.area === 'DB' ? 'dint' : 'dword' };
  return address;
}

function siemensS7ValueType(dataType: SiemensS7DataType, currentValueType: string): string {
  if (dataType === 'bit') return 'bool';
  if (dataType === 'real') return currentValueType === 'float64' ? 'float64' : 'float32';
  return currentValueType === 'int64' ? 'int64' : 'int32';
}

function defaultOmronFinsAddress(valueType: string): OmronFinsAddressSpec {
  return valueType === 'bool'
    ? { area: 'CIO', bit: 0, word: 0 }
    : { area: 'D', word: 100 };
}

function tryParseOmronFinsAddress(value: string): OmronFinsAddressSpec | undefined {
  const normalized = value.trim().toUpperCase().replace(/\s/g, '');
  const match = normalized.match(/^(CIO|WR|HR|DM|AR|W|H|D|A)(\d+)(?:\.(\d+))?$/);
  if (!match) return undefined;
  const areaByPrefix: Record<string, OmronFinsArea> = {
    CIO: 'CIO', W: 'W', WR: 'W', H: 'H', HR: 'H', D: 'D', DM: 'D', A: 'A', AR: 'A',
  };
  const area = areaByPrefix[match[1]];
  const word = Number(match[2]);
  const bit = match[3] === undefined ? undefined : Number(match[3]);
  if (!area || !Number.isInteger(word) || word < 0 || word >= omronFinsWordCapacity(area)) return undefined;
  if (bit !== undefined && (bit < 0 || bit > 15 || area === 'D')) return undefined;
  return { area, bit, word };
}

function parseOmronFinsAddress(value: string, valueType: string): OmronFinsAddressSpec {
  return tryParseOmronFinsAddress(value) ?? defaultOmronFinsAddress(valueType);
}

function serializeOmronFinsAddress(address: OmronFinsAddressSpec): string {
  const word = Math.min(
    omronFinsWordCapacity(address.area) - 1,
    Math.max(0, Math.trunc(address.word || 0)),
  );
  const prefix = address.area;
  if (address.bit === undefined || address.area === 'D') return `${prefix}${word}`;
  const bit = Math.min(15, Math.max(0, Math.trunc(address.bit || 0)));
  return `${prefix}${word}.${bit}`;
}

function normalizeOmronFinsForValueType(address: OmronFinsAddressSpec, valueType: string): OmronFinsAddressSpec {
  if (valueType === 'bool') {
    return { ...address, area: address.area === 'D' ? 'CIO' : address.area, bit: address.bit ?? 0 };
  }
  return { ...address, bit: undefined };
}

function omronFinsWordCapacity(area: OmronFinsArea): number {
  if (area === 'W' || area === 'H') return 512;
  if (area === 'A') return 1_024;
  return 4_096;
}

function defaultAddressForProtocol(protocol: string): PointSetPointResponse['address'] {
  if (protocol === 'CustomSerial') {
    return { kind: 'custom_serial_frame', value: serializeCustomSerialFrame(defaultCustomSerialFrame()) };
  }
  if (protocol === 'Dlt645') return { kind: 'dlt645_address', value: '' };
  if (protocol === 'Iec101') return { kind: 'iec101_ioa', value: '' };
  if (protocol === 'Iec104') return { kind: 'iec104_ioa', value: '' };
  if (protocol === 'OpcUa') return { kind: 'node_id', value: '' };
  if (protocol === 'BacnetIp') return { kind: 'bacnet_object_property', value: '42:analog_input:0:present_value' };
  if (protocol === 'SiemensS7') return { kind: 's7_address', value: 'DB1.REAL0' };
  if (protocol === 'OmronFins') return { kind: 'fins_address', value: 'D100' };
  if (protocol === 'Simulated') return { kind: 'simulated', value: '' };
  return {
    kind: 'holding_register',
    value: '',
    modbus: isModbusProtocol(protocol) ? defaultModbusOptions('float32') : undefined,
  };
}

function addressKindOptions(protocol: string): Array<[string, string]> {
  switch (protocol) {
    case 'ModbusRtu':
    case 'ModbusTcp':
      return [
        ['holding_register', '保持寄存器'],
        ['input_register', '输入寄存器'],
        ['coil', '线圈'],
        ['discrete_input', '离散输入'],
      ];
    case 'Dlt645':
      return [['dlt645_address', 'DL/T 645 数据标识']];
    case 'Iec101':
      return [['iec101_ioa', 'IEC 101 信息体地址']];
    case 'Iec104':
      return [['iec104_ioa', 'IEC 104 公共地址 / 信息体地址']];
    case 'OpcUa':
      return [
        ['node_id', '节点 ID'],
        ['browse_path', '语义路径'],
      ];
    case 'BacnetIp':
      return [['bacnet_object_property', 'BACnet 对象属性']];
    case 'SiemensS7':
      return [['s7_address', 'Siemens S7 地址']];
    case 'OmronFins':
      return [['fins_address', 'Omron FINS 地址']];
    case 'Simulated':
      return [['simulated', '模拟点位']];
    default:
      return [['holding_register', '协议地址']];
  }
}

function addressValuePlaceholder(protocol: string): string {
  switch (protocol) {
    case 'Iec104':
      return '1:1001';
    case 'Iec101':
      return '1001';
    case 'OpcUa':
      return 'ns=2;s=Machine/Temperature';
    case 'BacnetIp':
      return '42:analog_input:0:present_value';
    case 'Dlt645':
      return '123456789012:02010100:1';
    case 'SiemensS7':
      return 'DB1.REAL0';
    case 'OmronFins':
      return 'D100';
    default:
      return '40001';
  }
}

function parseBacnetAddressValue(value: string) {
  const [
    deviceInstance = '42',
    objectType = 'analog_input',
    objectInstance = '0',
    property = 'present_value',
    arrayIndex = '',
  ] = value.split(':');
  return { deviceInstance, objectType, objectInstance, property, arrayIndex };
}

function serializeBacnetAddressValue({
  arrayIndex,
  deviceInstance,
  objectInstance,
  objectType,
  property,
}: ReturnType<typeof parseBacnetAddressValue>): string {
  const base = `${deviceInstance}:${objectType}:${objectInstance}:${property}`;
  return arrayIndex === '' ? base : `${base}:${arrayIndex}`;
}

function parseDlt645AddressValue(value: string) {
  const [meterAddress = '', dataIdentifier = '', decimalPlaces = '', valueBytes = ''] = value.split(':');
  return { meterAddress, dataIdentifier, decimalPlaces, valueBytes };
}

function serializeDlt645AddressValue({
  dataIdentifier,
  decimalPlaces,
  meterAddress,
  valueBytes,
}: ReturnType<typeof parseDlt645AddressValue>): string {
  if (!meterAddress && !dataIdentifier && !decimalPlaces && !valueBytes) return '';
  const base = `${meterAddress}:${dataIdentifier}`;
  if (valueBytes !== '') return `${base}:${decimalPlaces}:${valueBytes}`;
  return decimalPlaces === '' ? base : `${base}:${decimalPlaces}`;
}

function pointSetEditorValueType(valueType: Dlt645DataIdentifierTemplateResponse['valueType']): string {
  switch (valueType) {
    case 'Integer': return 'int64';
    case 'Boolean': return 'bool';
    case 'Text': return 'string';
    default: return 'float32';
  }
}

function isModbusProtocol(protocol: string): boolean {
  return protocol === 'ModbusRtu' || protocol === 'ModbusTcp';
}

function isModbusRegisterAddress(kind: string): boolean {
  return kind === 'holding_register' || kind === 'input_register';
}

function defaultModbusOptions(valueType: string): NonNullable<PointSetPointResponse['address']['modbus']> {
  return {
    encoding: defaultModbusEncoding(valueType),
    byteOrder: 'big_endian',
    wordOrder: 'high_word_first',
    scale: 1,
    offset: 0,
  };
}

function defaultModbusEncoding(valueType: string): NonNullable<PointSetPointResponse['address']['modbus']>['encoding'] {
  if (valueType === 'float64') return 'f64';
  if (valueType === 'float32') return 'f32';
  if (valueType === 'int64') return 'i64';
  if (valueType === 'int32') return 'i32';
  return undefined;
}

function modbusEncodingOptions(valueType: string): Array<[string, string]> {
  if (valueType.startsWith('float')) {
    return [['', '自动'], ['f32', 'Float 32 位'], ['f64', 'Float 64 位']];
  }
  return [
    ['', '自动'], ['u16', '无符号 16 位'], ['i16', '有符号 16 位'],
    ['u32', '无符号 32 位'], ['i32', '有符号 32 位'],
    ['u64', '无符号 64 位'], ['i64', '有符号 64 位'],
  ];
}

function defaultOpcUaWriteDataType(valueType: string): NonNullable<PointSetPointResponse['opcUa']>['writeDataType'] {
  if (valueType === 'bool') return 'Boolean';
  if (valueType === 'string') return 'String';
  if (valueType === 'float64') return 'Double';
  if (valueType === 'float32') return 'Float';
  if (valueType === 'int64') return 'Int64';
  return 'Int32';
}

function opcUaWriteDataTypeOptions(valueType: string): Array<[NonNullable<PointSetPointResponse['opcUa']>['writeDataType'], string]> {
  if (valueType === 'bool') return [['Boolean', 'Boolean（布尔）']];
  if (valueType === 'string') return [['String', 'String（文本）']];
  if (valueType.startsWith('float')) {
    return [['Float', 'Float（32 位）'], ['Double', 'Double（64 位）']];
  }
  return [
    ['SByte', 'SByte（有符号 8 位）'],
    ['Byte', 'Byte（无符号 8 位）'],
    ['Int16', 'Int16'],
    ['UInt16', 'UInt16'],
    ['Int32', 'Int32'],
    ['UInt32', 'UInt32'],
    ['Int64', 'Int64'],
    ['UInt64', 'UInt64'],
  ];
}

function defaultIec104ControlType(valueType: string): NonNullable<PointSetPointResponse['iec104']>['controlType'] | undefined {
  if (valueType === 'bool') return 'C_SC_NA_1';
  if (valueType.startsWith('int')) return 'C_DC_NA_1';
  if (valueType.startsWith('float')) return 'C_SE_NC_1';
  return undefined;
}

function defaultIec101ControlType(valueType: string): NonNullable<PointSetPointResponse['iec101']>['controlType'] | undefined {
  return defaultIec104ControlType(valueType);
}

function defaultIec101PointOptions(
  valueType: string,
  selectBeforeOperate = false,
): PointSetPointResponse['iec101'] {
  const controlType = defaultIec101ControlType(valueType);
  return controlType ? { controlType, selectBeforeOperate } : undefined;
}

function iec101ControlTypeOptions(valueType: string): Array<[NonNullable<PointSetPointResponse['iec101']>['controlType'], string]> {
  return iec104ControlTypeOptions(valueType);
}

function defaultIec104PointOptions(
  valueType: string,
  selectBeforeOperate = false,
): PointSetPointResponse['iec104'] {
  const controlType = defaultIec104ControlType(valueType);
  return controlType ? { controlType, selectBeforeOperate } : undefined;
}

function iec104ControlTypeOptions(valueType: string): Array<[NonNullable<PointSetPointResponse['iec104']>['controlType'], string]> {
  if (valueType === 'bool') return [['C_SC_NA_1', 'C_SC_NA_1 单点遥控']];
  if (valueType.startsWith('int')) return [['C_DC_NA_1', 'C_DC_NA_1 双点遥控']];
  if (valueType.startsWith('float')) return [['C_SE_NC_1', 'C_SE_NC_1 短浮点设值']];
  return [];
}

function defaultCustomSerialFrame(): CustomSerialFrameSpec {
  return {
    frameEncoding: 'raw',
    offset: 0,
    requestChecksum: 'none',
    requestHex: '',
    responseChecksum: 'none',
    responsePrefixHex: '',
    scale: 1,
    schemaVersion: 2,
    valueEncoding: 'u16_be',
    valueOffset: 0,
  };
}

function parseCustomSerialFrame(value: string): CustomSerialFrameSpec {
  try {
    const parsed = JSON.parse(value) as Partial<CustomSerialFrameSpec>;
    return {
      ...defaultCustomSerialFrame(),
      ...parsed,
      schemaVersion: typeof parsed.schemaVersion === 'number' ? parsed.schemaVersion : 1,
      frameEncoding: typeof parsed.frameEncoding === 'string' ? parsed.frameEncoding : 'raw',
    };
  } catch {
    return defaultCustomSerialFrame();
  }
}

function defaultOpcUaBrowsePath(): OpcUaBrowsePathSpec {
  return {
    startingNode: 'i=85',
    elements: [{ namespaceIndex: 2, targetName: '' }],
  };
}

function parseOpcUaBrowsePath(value: string): OpcUaBrowsePathSpec {
  try {
    const parsed = JSON.parse(value) as Partial<OpcUaBrowsePathSpec>;
    if (typeof parsed.startingNode !== 'string' || !Array.isArray(parsed.elements)) {
      return defaultOpcUaBrowsePath();
    }
    return {
      startingNode: parsed.startingNode,
      elements: parsed.elements.map((element) => ({
        namespaceIndex: Number(element.namespaceIndex),
        targetName: String(element.targetName ?? ''),
      })),
    };
  } catch {
    return defaultOpcUaBrowsePath();
  }
}

function serializeOpcUaBrowsePath(path: OpcUaBrowsePathSpec): string {
  return JSON.stringify(path);
}

function opcUaBrowsePathSummary(value: string): string {
  const path = parseOpcUaBrowsePath(value);
  return [path.startingNode, ...path.elements.map((element) => `${element.namespaceIndex}:${element.targetName || '?'}`)].join(' → ');
}

function serializeCustomSerialFrame(frame: CustomSerialFrameSpec): string {
  const value: Record<string, unknown> = {
    schemaVersion: frame.schemaVersion,
    frameEncoding: frame.frameEncoding,
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

function validatePointSet(
  pointSet: PointSetEditorState,
  dlt645DataIdentifiers: Dlt645DataIdentifierTemplateResponse[],
): string | undefined {
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
      if (![1, 2].includes(frame.schemaVersion)) return `第 ${index + 1} 个点位的 DSL 版本只支持 v1 或 v2`;
      if (frame.schemaVersion === 1 && frame.frameEncoding !== 'raw') return `第 ${index + 1} 个点位的 DSL v1 只支持 Raw 原始帧`;
      if (!frame.requestHex.trim()) return `请填写第 ${index + 1} 个点位的请求帧`;
      if (!/^(?:0x)?[0-9a-fA-F\s:-]+$/.test(frame.requestHex) || frame.requestHex.replace(/^(?:0x)/i, '').replace(/[\s:-]/g, '').length % 2 !== 0) return `第 ${index + 1} 个点位的请求帧 HEX 格式不正确`;
      if (frame.valueEncoding === 'utf8' && (!frame.valueLength || frame.valueLength < 1)) return `第 ${index + 1} 个点位的文本长度必须大于 0`;
    }
    if (pointSet.protocol === 'Dlt645') {
      const address = parseDlt645AddressValue(point.address.value);
      if (!/^\d{12}$/.test(address.meterAddress)) return `第 ${index + 1} 个 DL/T 645 电表通信地址必须是 12 位数字`;
      if (!/^[0-9A-Fa-f]{8}$/.test(address.dataIdentifier)) return `第 ${index + 1} 个 DL/T 645 数据标识必须是 8 位十六进制`;
      const decimalPlaces = address.decimalPlaces === '' ? 0 : Number(address.decimalPlaces);
      if (!Number.isInteger(decimalPlaces) || decimalPlaces < 0 || decimalPlaces > 18) return `第 ${index + 1} 个 DL/T 645 小数位必须是 0-18 的整数`;
      const template = dlt645DataIdentifiers.find(
        (candidate) => candidate.dataIdentifier.toUpperCase() === address.dataIdentifier.toUpperCase(),
      );
      if (!template) {
        const valueBytes = Number(address.valueBytes);
        if (!Number.isInteger(valueBytes) || valueBytes < 1 || valueBytes > 251) return `第 ${index + 1} 个 DL/T 645 厂商数据标识必须填写 1-251 的响应值字节数`;
      } else if (address.valueBytes !== '' && Number(address.valueBytes) !== template.valueBytes) {
        return `第 ${index + 1} 个 DL/T 645 标准数据标识的响应值长度必须是 ${template.valueBytes} 字节`;
      }
    }
    if (pointSet.protocol === 'OpcUa' && point.address.kind === 'browse_path') {
      const path = parseOpcUaBrowsePath(point.address.value);
      if (!path.startingNode.trim()) return `请填写第 ${index + 1} 个点位的起始 NodeId`;
      if (path.elements.length === 0 || path.elements.some((element) => !Number.isInteger(element.namespaceIndex) || element.namespaceIndex < 0 || !element.targetName.trim())) return `请补全第 ${index + 1} 个点位的 OPC UA 语义路径`;
    }
    if (pointSet.protocol === 'OpcUa' && point.access !== 'read_only') {
      const dataType = point.opcUa?.writeDataType;
      if (!dataType) return `请选择第 ${index + 1} 个 OPC UA 可写点位的 UA 写入类型`;
      if (!opcUaWriteDataTypeOptions(point.valueType).some(([candidate]) => candidate === dataType)) return `第 ${index + 1} 个 OPC UA 写入类型与点位数据类型不匹配`;
    }
    if (pointSet.protocol === 'Iec104' && point.access !== 'read_only') {
      const controlType = point.iec104?.controlType;
      if (!controlType) return `请选择第 ${index + 1} 个 IEC 104 可写点位的控制类型`;
      if (!iec104ControlTypeOptions(point.valueType).some(([candidate]) => candidate === controlType)) return `第 ${index + 1} 个 IEC 104 控制类型与点位数据类型不匹配`;
    }
    if (pointSet.protocol === 'Iec101' && point.access !== 'read_only') {
      const controlType = point.iec101?.controlType;
      if (!controlType) return `请选择第 ${index + 1} 个 IEC 101 可写点位的控制类型`;
      if (!iec101ControlTypeOptions(point.valueType).some(([candidate]) => candidate === controlType)) return `第 ${index + 1} 个 IEC 101 控制类型与点位数据类型不匹配`;
    }
    if (pointSet.protocol === 'SiemensS7') {
      const address = tryParseSiemensS7Address(point.address.value);
      if (point.address.kind !== 's7_address' || !address) return `第 ${index + 1} 个 Siemens S7 地址无效`;
      const compatible = point.valueType === 'bool'
        ? address.dataType === 'bit'
        : point.valueType.startsWith('float')
          ? address.dataType === 'real' || address.dataType === 'dword'
          : point.valueType.startsWith('int')
            ? ['byte', 'word', 'dword', 'int', 'dint'].includes(address.dataType)
            : false;
      if (!compatible) return `第 ${index + 1} 个 Siemens S7 地址格式与点位数据类型不匹配`;
      if (address.area === 'I' && point.access !== 'read_only') return `第 ${index + 1} 个 Siemens S7 过程输入点只能配置为只读`;
    }
    if (pointSet.protocol === 'OmronFins') {
      const address = tryParseOmronFinsAddress(point.address.value);
      if (point.address.kind !== 'fins_address' || !address) return `第 ${index + 1} 个 Omron FINS 地址无效`;
      if (point.valueType === 'bool' && address.bit === undefined) return `第 ${index + 1} 个 Omron FINS 布尔点必须配置位号`;
      if ((point.valueType.startsWith('float') || point.valueType.startsWith('int')) && address.bit !== undefined) return `第 ${index + 1} 个 Omron FINS 数值点不能配置位号`;
      if (point.valueType === 'string') return `第 ${index + 1} 个 Omron FINS 点位暂不支持字符串类型`;
      if (point.valueType.startsWith('float') && address.word + 2 > omronFinsWordCapacity(address.area)) return `第 ${index + 1} 个 Omron FINS 浮点地址超出存储区边界`;
    }
    if (isModbusProtocol(pointSet.protocol) && isModbusRegisterAddress(point.address.kind)) {
      const modbus = point.address.modbus ?? defaultModbusOptions(point.valueType);
      if (!Number.isFinite(modbus.scale) || modbus.scale === 0 || !Number.isFinite(modbus.offset)) return `第 ${index + 1} 个点位的缩放系数必须非零且缩放、偏移必须为有限数值`;
      if (modbus.bitIndex !== undefined && (!Number.isInteger(modbus.bitIndex) || modbus.bitIndex < 0 || modbus.bitIndex > 15)) return `第 ${index + 1} 个点位的寄存器位必须是 0-15 的整数`;
      if (modbus.bitIndex !== undefined && point.access !== 'read_only') return `第 ${index + 1} 个寄存器位点位暂不支持写入`;
    }
    if (pointSet.protocol === 'BacnetIp') {
      const address = parseBacnetAddressValue(point.address.value);
      const deviceInstance = Number(address.deviceInstance);
      const objectInstance = Number(address.objectInstance);
      const arrayIndex = address.arrayIndex === '' ? undefined : Number(address.arrayIndex);
      const writePriority = point.bacnet?.writePriority ?? 16;
      if (!Number.isInteger(deviceInstance) || deviceInstance < 0 || deviceInstance > 4_194_302) return `第 ${index + 1} 个 BACnet 设备实例号无效`;
      if (!address.objectType || !Number.isInteger(objectInstance) || objectInstance < 0 || objectInstance > 4_194_302) return `第 ${index + 1} 个 BACnet 对象地址无效`;
      if (!address.property || (arrayIndex !== undefined && (!Number.isInteger(arrayIndex) || arrayIndex < 0))) return `第 ${index + 1} 个 BACnet 属性地址无效`;
      if (!Number.isInteger(writePriority) || writePriority < 1 || writePriority > 16) return `第 ${index + 1} 个 BACnet 写入优先级必须是 1-16 的整数`;
      if (!bacnetObjectIsWritable(address.objectType) && point.access !== 'read_only') return `第 ${index + 1} 个 BACnet 输入对象只能配置为只读`;
      if (point.access !== 'read_only' && address.property !== 'present_value') return `第 ${index + 1} 个 BACnet 可写点位必须使用 present_value 属性`;
      if (point.access !== 'read_only' && arrayIndex !== undefined) return `第 ${index + 1} 个 BACnet 可写点位不能指定数组索引`;
      if (point.access !== 'read_only' && ['analog_output', 'analog_value'].includes(address.objectType) && !point.valueType.startsWith('float')) return `第 ${index + 1} 个 BACnet 模拟量写入点必须使用浮点类型`;
      if (point.access !== 'read_only' && ['binary_output', 'binary_value'].includes(address.objectType) && point.valueType !== 'bool') return `第 ${index + 1} 个 BACnet 开关量写入点必须使用 bool 类型`;
      if (point.access !== 'read_only' && ['multi_state_output', 'multi_state_value'].includes(address.objectType) && !point.valueType.startsWith('int')) return `第 ${index + 1} 个 BACnet 多状态写入点必须使用整数类型`;
    }
    if (ids.has(point.pointId)) return `点位 ID ${point.pointId} 重复`;
    if (point.intervalMs < 1) return `第 ${index + 1} 个点位采集周期必须大于 0`;
    ids.add(point.pointId);
  }
  return undefined;
}

function bacnetObjectIsWritable(objectType: string): boolean {
  return ['analog_output', 'analog_value', 'binary_output', 'binary_value', 'multi_state_output', 'multi_state_value'].includes(objectType);
}

function protocolLabel(
  protocol: string,
  protocolOptions: Array<readonly [string, string]>,
): string {
  return protocolOptions.find(([value]) => value === protocol)?.[1] ?? protocol;
}

function intervalSummary(points: PointSetPointResponse[]): string {
  const intervals = Array.from(new Set(points.map((point) => point.intervalMs)));
  if (intervals.length === 0) return '-';
  if (intervals.length === 1) return `${intervals[0]}ms`;
  return `${Math.min(...intervals)}-${Math.max(...intervals)}ms`;
}
