import { useEffect, useState } from 'react';
import { Edit3, Plus, ShieldCheck, Trash2, X } from 'lucide-react';

import type {
  EdgeNodeResponse,
  ManagementActionResponse,
  ProtocolConnectionResponse,
  CreateProtocolConnectionRequest,
  SaveProtocolConnectionRequest,
} from '../api/types';
import { Drawer } from '../components/Drawer';
import { Modal } from '../components/Modal';
import { PaginationBar } from '../components/PaginationBar';
import { displayError } from '../utils/errors';
import './PointMappingsPage.css';

const emptyConnection: ProtocolConnectionResponse = {
    edgeId: '',
    connectionId: '',
    protocol: '',
    protocolType: 'ModbusTcp',
    endpoint: '',
    status: '停用',
    policy: '',
};

const emptyEdge: EdgeNodeResponse = {
  edgeId: '', displayName: '未选择边端', site: '-', runtimeId: '-', status: '未接入',
  resources: '-', heartbeat: '-', capabilities: [],
};

const protocolOptions = [
  ['ModbusTcp', 'Modbus TCP'],
  ['ModbusRtu', 'Modbus RTU'],
  ['Dlt645', 'DL/T645'],
  ['Iec101', 'IEC-101'],
  ['CustomSerial', '自定义串口'],
  ['OpcUa', 'OPC UA'],
  ['SiemensS7', 'Siemens S7'],
  ['Simulated', 'Simulated'],
];

export function ProtocolConnectionsPage({
  connections = [],
  edges = [],
  embedded = false,
  mode = 'configure',
  onCreateConnection,
  onDeleteConnection,
  onSaveConnection,
  onValidateConnection,
  selectedEdgeId = edges[0]?.edgeId ?? '',
}: {
  connections?: ProtocolConnectionResponse[];
  edges?: EdgeNodeResponse[];
  embedded?: boolean;
  mode?: 'configure' | 'list';
  onCreateConnection?: (
    edgeId: string,
    request: CreateProtocolConnectionRequest,
  ) => Promise<ProtocolConnectionResponse> | ProtocolConnectionResponse;
  onDeleteConnection?: (
    edgeId: string,
    connectionId: string,
  ) => Promise<void> | void;
  onSaveConnection?: (
    edgeId: string,
    connectionId: string,
    request: SaveProtocolConnectionRequest,
  ) => Promise<void> | void;
  onValidateConnection?: (
    edgeId: string,
  ) => Promise<ManagementActionResponse> | ManagementActionResponse;
  selectedEdgeId?: string;
}) {
  const [selectedConnectionId, setSelectedConnectionId] = useState(
    () => connections[0]?.connectionId ?? '',
  );
  const [page, setPage] = useState(1);
  const selectedConnection =
    connections.find((connection) => connection.connectionId === selectedConnectionId) ??
    connections[0] ??
    emptyConnection;
  const [form, setForm] = useState(() => connectionToEditorForm(selectedConnection));
  const [saveState, setSaveState] = useState<'idle' | 'saving' | 'saved' | 'error'>(
    'idle',
  );
  const [createState, setCreateState] = useState<'idle' | 'creating'>('idle');
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [editDialogOpen, setEditDialogOpen] = useState(false);
  const [createForm, setCreateForm] = useState<CreateProtocolConnectionRequest>({
    endpoint: '/dev/ttyUSB0',
    protocolType: 'ModbusRtu',
    serial: serialDefaultsForProtocol('ModbusRtu', '/dev/ttyUSB0'),
  });
  const [validateState, setValidateState] = useState<'idle' | 'validating'>('idle');
  const [toolbarMessage, setToolbarMessage] = useState('');
  const isConfigureMode = mode === 'configure';
  const pageSize = 10;
  const totalPages = Math.max(1, Math.ceil(connections.length / pageSize));
  const currentPage = Math.min(page, totalPages);
  const visibleConnections = connections.slice(
    (currentPage - 1) * pageSize,
    currentPage * pageSize,
  );
  const activeEdge =
    edges.find((edge) => edge.edgeId === selectedEdgeId) ?? edges[0] ?? emptyEdge;

  useEffect(() => {
    setForm(connectionToEditorForm(selectedConnection));
    setSaveState((current) =>
      current === 'saving' || current === 'saved' ? current : 'idle',
    );
  }, [selectedConnection]);

  useEffect(() => {
    if (
      connections.length > 0 &&
      !connections.some((connection) => connection.connectionId === selectedConnectionId)
    ) {
      setSelectedConnectionId(connections[0].connectionId);
    }
  }, [connections, selectedConnectionId]);

  useEffect(() => {
    setPage(1);
  }, [selectedEdgeId]);

  const handleSave = async () => {
    setSaveState('saving');

    try {
      await onSaveConnection?.(selectedEdgeId, selectedConnection.connectionId, {
        ...requestFromEditor(form),
      });
      setSaveState('saved');
    } catch (error) {
      setSaveState('error');
      setToolbarMessage(`保存连接失败：${displayError(error)}`);
    }
  };

  const handleCreate = async () => {
    setCreateState('creating');
    setSaveState('idle');
    setToolbarMessage('');

    try {
      const created = await onCreateConnection?.(selectedEdgeId, {
        endpoint: createForm.endpoint?.trim() || null,
        protocolType: createForm.protocolType,
        serial: isSerialProtocol(createForm.protocolType)
          ? createForm.serial ??
            serialDefaultsForProtocol(
              createForm.protocolType,
              createForm.endpoint ?? '/dev/ttyUSB0',
            )
          : null,
      });
      if (created) {
        setSelectedConnectionId(created.connectionId);
        setToolbarMessage(`已创建连接 ${created.connectionId}`);
      } else {
        setToolbarMessage('已创建连接');
      }
      setCreateDialogOpen(false);
    } catch (error) {
      setToolbarMessage(`创建连接失败：${displayError(error)}`);
    } finally {
      setCreateState('idle');
    }
  };

  const handleValidateConnection = async () => {
    setValidateState('validating');
    setToolbarMessage('');

    try {
      if (!onValidateConnection) {
        setToolbarMessage('连接校验未接入后端');
        return;
      }

      const result = await onValidateConnection(selectedEdgeId);
      setToolbarMessage(
        result.status ? `连接校验 ${result.status}` : '连接校验无结果',
      );
    } catch (error) {
      setToolbarMessage(`连接校验失败：${displayError(error)}`);
    } finally {
      setValidateState('idle');
    }
  };

  const handleDelete = async (connectionId: string) => {
    setToolbarMessage('');
    try {
      await onDeleteConnection?.(selectedEdgeId, connectionId);
      setToolbarMessage(`已删除连接 ${connectionId}`);
      setEditDialogOpen(false);
    } catch (error) {
      setToolbarMessage(`删除连接失败：${displayError(error, '请先解除点位或数据配置引用')}`);
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>协议连接实例</h2>
          <p>串口、Modbus 与 DL/T645 采集通道。</p>
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
                disabled={validateState === 'validating' || !selectedEdgeId}
                onClick={() => {
                  void handleValidateConnection();
                }}
                type="button"
              >
                <ShieldCheck size={15} aria-hidden="true" />
                {validateState === 'validating' ? '校验中' : '校验连接'}
              </button>
              <button
                className="primary-button"
                disabled={createState === 'creating' || !selectedEdgeId}
                onClick={() => setCreateDialogOpen(true)}
                type="button"
              >
                <Plus size={15} aria-hidden="true" />
                新建连接
              </button>
            </>
          ) : null}
        </div>
      </section>

      {createDialogOpen ? (
        <Modal onClose={() => setCreateDialogOpen(false)}>
          <form
            aria-labelledby="protocol-create-dialog-title"
            className="modal-panel"
            onSubmit={(event) => {
              event.preventDefault();
              void handleCreate();
            }}
            role="dialog"
          >
            <div className="modal-header">
              <h3 id="protocol-create-dialog-title">新建协议连接</h3>
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
                <span>协议类型</span>
                <select
                  aria-label="新建协议类型"
                  value={createForm.protocolType ?? 'ModbusRtu'}
                  onChange={(event) => {
                    const protocolType = event.target.value;
                    setCreateForm((current) => {
                      const endpoint = current.endpoint ?? '/dev/ttyUSB0';
                      return {
                        ...current,
                        endpoint,
                        protocolType,
                        serial: isSerialProtocol(protocolType)
                          ? serialDefaultsForProtocol(protocolType, endpoint)
                          : null,
                      };
                    });
                  }}
                >
                  {protocolOptions.map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>
              </label>
              {isSerialProtocol(createForm.protocolType) ? (
                <>
                  <label>
                    <span>串口设备</span>
                    <input
                      aria-label="新建串口设备"
                      value={createForm.serial?.port ?? createForm.endpoint ?? ''}
                      onChange={(event) => {
                        const port = event.target.value;
                        setCreateForm((current) => ({
                          ...current,
                          endpoint: port,
                          serial: {
                            ...(current.serial ??
                              serialDefaultsForProtocol(current.protocolType, port)),
                            port,
                          },
                        }));
                      }}
                    />
                  </label>
                  <SerialFields
                    idPrefix="新建"
                    serial={
                      createForm.serial ??
                      serialDefaultsForProtocol(
                        createForm.protocolType,
                        createForm.endpoint ?? '/dev/ttyUSB0',
                      )
                    }
                    onChange={(serial) =>
                      setCreateForm((current) => ({
                        ...current,
                        endpoint: serial.port,
                        serial,
                      }))
                    }
                  />
                </>
              ) : (
                <label>
                  <span>端点</span>
                  <input
                    aria-label="新建端点"
                    value={createForm.endpoint ?? ''}
                    onChange={(event) =>
                      setCreateForm((current) => ({
                        ...current,
                        endpoint: event.target.value,
                      }))
                    }
                  />
                </label>
              )}
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
                disabled={createState === 'creating'}
                type="submit"
              >
                {createState === 'creating' ? '保存中' : '保存'}
              </button>
            </div>
          </form>
        </Modal>
      ) : null}

      <div className={isConfigureMode ? 'point-config-layout' : 'point-config-layout list-only'}>
        <section className="panel point-table-panel">
          <div className="panel-header">
            <h3>连接清单</h3>
            <span>
              {activeEdge.displayName} · {connections.length} 个连接
            </span>
          </div>
          <div className="table-wrap">
            <table className="ops-table">
              <thead>
                <tr>
                  <th>Connection ID</th>
                  <th>协议</th>
                  <th>端点</th>
                  <th>状态</th>
                  <th>策略</th>
                  {isConfigureMode ? <th>操作</th> : null}
                </tr>
              </thead>
              <tbody>
                {visibleConnections.length === 0 ? (
                  <tr>
                    <td className="table-empty-cell" colSpan={isConfigureMode ? 6 : 5}>
                      暂无协议连接
                    </td>
                  </tr>
                ) : null}
                {visibleConnections.map((connection) => (
                  <tr key={`${connection.edgeId}:${connection.connectionId}`}>
                    <td>
                      {isConfigureMode ? (
                        <button
                          aria-label={`编辑连接 ${connection.connectionId}`}
                          aria-pressed={
                            connection.connectionId === selectedConnection.connectionId
                          }
                          className="point-id-button"
                          onClick={() => {
                            setSelectedConnectionId(connection.connectionId);
                            setEditDialogOpen(true);
                          }}
                          type="button"
                        >
                          {connection.connectionId}
                        </button>
                      ) : (
                        connection.connectionId
                      )}
                    </td>
                    <td>{connection.protocol}</td>
                    <td>{connection.endpoint}</td>
                    <td>
                      <span className={connection.status === '启用' ? 'tag ok' : 'tag warn'}>
                        {connection.status}
                      </span>
                    </td>
                    <td>{connection.policy}</td>
                    {isConfigureMode ? (
                      <td>
                        <div className="row-actions">
                          <button
                            aria-label={`修改连接 ${connection.connectionId}`}
                            className="secondary-button compact"
                            onClick={() => {
                              setSelectedConnectionId(connection.connectionId);
                              setEditDialogOpen(true);
                            }}
                            type="button"
                          >
                            <Edit3 size={14} aria-hidden="true" />
                            修改
                          </button>
                          <button
                            aria-label={`删除连接 ${connection.connectionId}`}
                            className="danger-button compact"
                            onClick={() => {
                              void handleDelete(connection.connectionId);
                            }}
                            type="button"
                          >
                            <Trash2 size={14} aria-hidden="true" />
                            删除
                          </button>
                        </div>
                      </td>
                    ) : null}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {connections.length > pageSize ? (
            <PaginationBar
              ariaLabel="协议连接分页"
              currentPage={currentPage}
              onNext={() => setPage((value) => Math.min(totalPages, value + 1))}
              onPrevious={() => setPage((value) => Math.max(1, value - 1))}
              totalPages={totalPages}
            />
          ) : null}
        </section>

        {isConfigureMode && editDialogOpen ? (
          <Drawer
          onClose={() => setEditDialogOpen(false)}
          subtitle="保存后进入待发布配置，发布后边端 runtime 重新建立协议会话"
          title={`编辑连接 ${selectedConnection.connectionId}`}
          footer={
            <>
              <span className={`editor-status ${saveState}`} role="status">
                {saveStatusText(saveState)}
              </span>
              <button
                className="secondary-button"
                onClick={() => {
                  setForm(connectionToEditorForm(selectedConnection));
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
          <section className="drawer-section">
            <h4>连接参数</h4>
            <div className="editor-grid">
              <label className="editor-control">
                <span>协议类型</span>
                <select
                  aria-label="协议类型"
                  value={form.protocolType}
                  onChange={(event) =>
                    setForm((current) =>
                      applyProtocolToEditorForm(current, event.target.value),
                    )
                  }
                >
                  {protocolOptions.map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>
              </label>
              {isSerialProtocol(form.protocolType) ? (
                <>
                  <label className="editor-control">
                    <span>串口设备</span>
                    <input
                      aria-label="串口设备"
                      value={form.serial.port}
                      onChange={(event) => {
                        const port = event.target.value;
                        setForm((current) => ({
                          ...current,
                          endpoint: port,
                          serial: { ...current.serial, port },
                        }));
                      }}
                    />
                  </label>
                  <SerialFields
                    serial={form.serial}
                    onChange={(serial) =>
                      setForm((current) => ({
                        ...current,
                        endpoint: serial.port,
                        serial,
                      }))
                    }
                  />
                </>
              ) : (
                <label className="editor-control">
                  <span>端点</span>
                  <input
                    aria-label="端点"
                    value={form.endpoint}
                    onChange={(event) =>
                      setForm((current) => ({
                        ...current,
                        endpoint: event.target.value,
                      }))
                    }
                  />
                </label>
              )}
            </div>
          </section>
          <DrawerSection
            fields={[
              ['边端', selectedEdgeId],
              ['Connection ID', selectedConnection.connectionId],
              ['当前协议', selectedConnection.protocol],
              ['当前状态', selectedConnection.status],
            ]}
            title="当前版本"
          />
          </Drawer>
        ) : null}
      </div>
    </div>
  );
}

interface EditorForm {
  endpoint: string;
  protocolType: string;
  serial: NonNullable<CreateProtocolConnectionRequest['serial']>;
}

function connectionToEditorForm(connection: ProtocolConnectionResponse): EditorForm {
  return {
    endpoint: connection.endpoint,
    protocolType: connection.protocolType || inferProtocolType(connection.protocol),
    serial:
      connection.serial ??
      serialDefaultsForProtocol(
        connection.protocolType || inferProtocolType(connection.protocol),
        connection.endpoint,
      ),
  };
}

function isSerialProtocol(protocolType: string | undefined): boolean {
  return ['ModbusRtu', 'Dlt645', 'Iec101', 'CustomSerial'].includes(protocolType ?? '');
}

function serialDefaultsForProtocol(
  protocolType: string | undefined,
  port: string,
): NonNullable<CreateProtocolConnectionRequest['serial']> {
  return {
    port,
    baudRate: protocolType === 'Dlt645' ? 2400 : 9600,
    dataBits: 8,
    stopBits: 1,
    parity: protocolType === 'Dlt645' || protocolType === 'Iec101' ? 'even' : 'none',
  };
}

function applyProtocolToEditorForm(form: EditorForm, protocolType: string): EditorForm {
  return {
    ...form,
    protocolType,
    serial: isSerialProtocol(protocolType)
      ? serialDefaultsForProtocol(protocolType, form.serial.port || form.endpoint)
      : form.serial,
  };
}

function requestFromEditor(form: EditorForm): SaveProtocolConnectionRequest {
  if (isSerialProtocol(form.protocolType)) {
    return {
      endpoint: form.serial.port.trim() || null,
      protocolType: form.protocolType,
      serial: { ...form.serial, port: form.serial.port.trim() },
    };
  }
  return {
    endpoint: form.endpoint.trim() || null,
    protocolType: form.protocolType,
  };
}

function SerialFields({
  idPrefix = '',
  onChange,
  serial,
}: {
  idPrefix?: string;
  onChange: (serial: NonNullable<CreateProtocolConnectionRequest['serial']>) => void;
  serial: NonNullable<CreateProtocolConnectionRequest['serial']>;
}) {
  const label = (value: string) => `${idPrefix}${value}`;
  return (
    <>
      <label className="editor-control">
        <span>波特率</span>
        <input
          aria-label={label('波特率')}
          min={1}
          onChange={(event) => onChange({ ...serial, baudRate: Number(event.target.value) })}
          type="number"
          value={serial.baudRate}
        />
      </label>
      <label className="editor-control">
        <span>数据位</span>
        <select
          aria-label={label('数据位')}
          onChange={(event) => onChange({ ...serial, dataBits: Number(event.target.value) })}
          value={serial.dataBits}
        >
          {[5, 6, 7, 8].map((value) => (
            <option key={value} value={value}>
              {value}
            </option>
          ))}
        </select>
      </label>
      <label className="editor-control">
        <span>停止位</span>
        <select
          aria-label={label('停止位')}
          onChange={(event) => onChange({ ...serial, stopBits: Number(event.target.value) })}
          value={serial.stopBits}
        >
          <option value={1}>1</option>
          <option value={2}>2</option>
        </select>
      </label>
      <label className="editor-control">
        <span>校验位</span>
        <select
          aria-label={label('校验位')}
          onChange={(event) =>
            onChange({
              ...serial,
              parity: event.target.value as 'none' | 'even' | 'odd',
            })
          }
          value={serial.parity}
        >
          <option value="none">无校验</option>
          <option value="even">偶校验</option>
          <option value="odd">奇校验</option>
        </select>
      </label>
    </>
  );
}

function inferProtocolType(protocol: string): string {
  const normalized = protocol.toLowerCase();
  if (normalized.includes('opc')) {
    return 'OpcUa';
  }
  if (normalized.includes('rtu') || normalized.includes('rs485')) {
    return 'ModbusRtu';
  }
  if (normalized.includes('s7') || normalized.includes('siemens')) {
    return 'SiemensS7';
  }
  if (normalized.includes('dlt') || normalized.includes('645')) {
    return 'Dlt645';
  }
  if (normalized.includes('iec')) {
    return 'Iec101';
  }
  if (normalized.includes('simulated')) {
    return 'Simulated';
  }
  return 'ModbusTcp';
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
