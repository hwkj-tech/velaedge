import { useEffect, useState } from 'react';
import { Plus, ShieldCheck, X } from 'lucide-react';

import type {
  EdgeNodeResponse,
  ManagementActionResponse,
  ProtocolConnectionResponse,
  CreateProtocolConnectionRequest,
  SaveProtocolConnectionRequest,
} from '../api/types';
import { Drawer } from '../components/Drawer';
import { PaginationBar } from '../components/PaginationBar';
import './PointMappingsPage.css';

const fallbackConnections: ProtocolConnectionResponse[] = [
  {
    edgeId: 'edge-dev',
    connectionId: 'modbus-line-a',
    protocol: 'Modbus TCP',
    protocolType: 'ModbusTcp',
    endpoint: '10.12.0.20:502',
    status: '启用',
    policy: '1000ms timeout / 3 retry',
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
  connections = fallbackConnections,
  edges = fallbackEdges,
  embedded = false,
  mode = 'configure',
  onCreateConnection,
  onSaveConnection,
  onValidateConnection,
  selectedEdgeId = edges[0]?.edgeId ?? 'edge-dev',
}: {
  connections?: ProtocolConnectionResponse[];
  edges?: EdgeNodeResponse[];
  embedded?: boolean;
  mode?: 'configure' | 'list';
  onCreateConnection?: (
    edgeId: string,
    request: CreateProtocolConnectionRequest,
  ) => Promise<ProtocolConnectionResponse> | ProtocolConnectionResponse;
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
    () => connections[0]?.connectionId ?? fallbackConnections[0].connectionId,
  );
  const [page, setPage] = useState(1);
  const selectedConnection =
    connections.find((connection) => connection.connectionId === selectedConnectionId) ??
    connections[0] ??
    fallbackConnections[0];
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
    edges.find((edge) => edge.edgeId === selectedEdgeId) ?? edges[0] ?? fallbackEdges[0];

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
        endpoint: form.endpoint.trim() || null,
        protocolType: form.protocolType,
      });
      setSaveState('saved');
    } catch {
      setSaveState('error');
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
      });
      if (created) {
        setSelectedConnectionId(created.connectionId);
        setToolbarMessage(`已创建连接 ${created.connectionId}`);
      } else {
        setToolbarMessage('已创建连接');
      }
      setCreateDialogOpen(false);
    } catch {
      setToolbarMessage('创建连接失败');
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
    } catch {
      setToolbarMessage('连接校验失败');
    } finally {
      setValidateState('idle');
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>协议连接实例</h2>
          <p>
            云端维护可复用连接模板，保存时做字段和密钥校验，边端收到配置后使用本地适配器建立真实连接。
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
                disabled={validateState === 'validating'}
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
                disabled={createState === 'creating'}
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
        <div className="modal-backdrop">
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
                  onChange={(event) =>
                    setCreateForm((current) => ({
                      ...current,
                      protocolType: event.target.value,
                    }))
                  }
                >
                  {protocolOptions.map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>
              </label>
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
        </div>
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
                </tr>
              </thead>
              <tbody>
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
                    setForm((current) => ({
                      ...current,
                      protocolType: event.target.value,
                    }))
                  }
                >
                  {protocolOptions.map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>
              </label>
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
}

function connectionToEditorForm(connection: ProtocolConnectionResponse): EditorForm {
  return {
    endpoint: connection.endpoint,
    protocolType: connection.protocolType || inferProtocolType(connection.protocol),
  };
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
