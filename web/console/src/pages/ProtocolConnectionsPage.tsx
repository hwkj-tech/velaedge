import { useEffect, useState } from 'react';
import { Edit3, Plus, ShieldCheck, Trash2, X } from 'lucide-react';

import type {
  BacnetIpConnectionSettings,
  EdgeNodeResponse,
  Iec101ConnectionSettings,
  Iec104ConnectionSettings,
  ManagementActionResponse,
  OmronFinsConnectionSettings,
  OpcUaConnectionSettings,
  ProtocolCircuitBreakerConfig,
  ProtocolConnectionResponse,
  RuntimeProtocolDescriptor,
  CreateProtocolConnectionRequest,
  SaveProtocolConnectionRequest,
  SiemensS7ConnectionSettings,
} from '../api/types';
import { Drawer } from '../components/Drawer';
import { Modal } from '../components/Modal';
import { PaginationBar } from '../components/PaginationBar';
import { displayError } from '../utils/errors';
import { protocolOptionsFromCatalog } from '../protocolCatalog';
import './PointMappingsPage.css';

const emptyConnection: ProtocolConnectionResponse = {
    edgeId: '',
    connectionId: '',
    protocol: '',
    protocolType: 'ModbusTcp',
    endpoint: '',
    circuitBreaker: defaultCircuitBreaker(),
    status: '停用',
    policy: '',
};

const emptyEdge: EdgeNodeResponse = {
  edgeId: '', displayName: '未选择边端', site: '-', runtimeId: '-', status: '未接入',
  resources: '-', heartbeat: '-', capabilities: [],
};

function defaultCircuitBreaker(): ProtocolCircuitBreakerConfig {
  return {
    enabled: true,
    failureThreshold: 5,
    openDurationMs: 30_000,
    halfOpenSuccessThreshold: 1,
  };
}

function defaultIec104Settings(): Iec104ConnectionSettings {
  return { cp56TimeZoneOffsetMinutes: 0 };
}

function defaultIec101Settings(): Iec101ConnectionSettings {
  return { cp56TimeZoneOffsetMinutes: 0 };
}

function defaultOpcUaSettings(): OpcUaConnectionSettings {
  return {
    securityPolicy: 'none',
    messageSecurityMode: 'none',
    authMode: 'anonymous',
    username: null,
    passwordEnv: null,
    userCertificatePath: null,
    userPrivateKeyPath: null,
    pkiDir: './data/opcua-pki',
    trustServerCerts: false,
    verifyServerCerts: true,
    connectTimeoutMs: 5_000,
    requestTimeoutMs: 5_000,
    sessionTimeoutMs: 60_000,
    sessionRetryLimit: 3,
  };
}

function defaultBacnetIpSettings(): BacnetIpConnectionSettings {
  return {
    bindAddress: '0.0.0.0',
    localPort: 0,
    broadcastAddress: '255.255.255.255',
    apduTimeoutMs: 3_000,
    apduRetries: 3,
    discoveryTimeoutMs: 1_000,
    maxApduLength: 1476,
    foreignDevice: null,
    cov: null,
  };
}

function defaultSiemensS7Settings(): SiemensS7ConnectionSettings {
  return {
    rack: 0,
    slot: 1,
    pduSize: 480,
    connectTimeoutMs: 5_000,
    requestTimeoutMs: 10_000,
  };
}

function defaultOmronFinsSettings(): OmronFinsConnectionSettings {
  return {
    transport: 'udp',
    sourceNetwork: 0,
    sourceNode: 1,
    sourceUnit: 0,
    destinationNetwork: 0,
    destinationNode: 0,
    destinationUnit: 0,
    timeoutMs: 2_000,
    wordOrder: 'low_word_first',
  };
}

export function ProtocolConnectionsPage({
  connections = [],
  edges = [],
  embedded = false,
  mode = 'configure',
  onCreateConnection,
  onDeleteConnection,
  onSaveConnection,
  onValidateConnection,
  protocolCatalog,
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
  protocolCatalog?: RuntimeProtocolDescriptor[];
  selectedEdgeId?: string;
}) {
  const protocolOptions = protocolOptionsFromCatalog(protocolCatalog);
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
    circuitBreaker: defaultCircuitBreaker(),
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
        ...(isIec101Protocol(createForm.protocolType)
          ? { iec101: createForm.iec101 ?? defaultIec101Settings() }
          : {}),
        ...(isIec104Protocol(createForm.protocolType)
          ? { iec104: createForm.iec104 ?? defaultIec104Settings() }
          : {}),
        opcUa: isOpcUaProtocol(createForm.protocolType)
          ? createForm.opcUa ?? defaultOpcUaSettings()
          : null,
        ...(isBacnetIpProtocol(createForm.protocolType)
          ? { bacnetIp: createForm.bacnetIp ?? defaultBacnetIpSettings() }
          : {}),
        ...(isSiemensS7Protocol(createForm.protocolType)
          ? { siemensS7: createForm.siemensS7 ?? defaultSiemensS7Settings() }
          : {}),
        ...(isOmronFinsProtocol(createForm.protocolType)
          ? { omronFins: createForm.omronFins ?? defaultOmronFinsSettings() }
          : {}),
        circuitBreaker: createForm.circuitBreaker ?? defaultCircuitBreaker(),
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
                      const endpoint = endpointForProtocol(protocolType, current.endpoint);
                      return {
                        ...current,
                        endpoint,
                        protocolType,
                        serial: isSerialProtocol(protocolType)
                          ? serialDefaultsForProtocol(protocolType, endpoint)
                          : null,
                        iec101: isIec101Protocol(protocolType)
                          ? current.iec101 ?? defaultIec101Settings()
                          : null,
                        iec104: isIec104Protocol(protocolType)
                          ? current.iec104 ?? defaultIec104Settings()
                          : null,
                        opcUa: isOpcUaProtocol(protocolType)
                          ? current.opcUa ?? defaultOpcUaSettings()
                          : null,
                        bacnetIp: isBacnetIpProtocol(protocolType)
                          ? current.bacnetIp ?? defaultBacnetIpSettings()
                          : null,
                        siemensS7: isSiemensS7Protocol(protocolType)
                          ? current.siemensS7 ?? defaultSiemensS7Settings()
                          : null,
                        omronFins: isOmronFinsProtocol(protocolType)
                          ? current.omronFins ?? defaultOmronFinsSettings()
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
              {isIec104Protocol(createForm.protocolType) ? (
                <Iec104Fields
                  idPrefix="新建"
                  value={createForm.iec104 ?? defaultIec104Settings()}
                  onChange={(iec104) =>
                    setCreateForm((current) => ({ ...current, iec104 }))
                  }
                />
              ) : null}
              {isIec101Protocol(createForm.protocolType) ? (
                <Iec101Fields
                  idPrefix="新建"
                  value={createForm.iec101 ?? defaultIec101Settings()}
                  onChange={(iec101) =>
                    setCreateForm((current) => ({ ...current, iec101 }))
                  }
                />
              ) : null}
              {isOpcUaProtocol(createForm.protocolType) ? (
                <OpcUaFields
                  idPrefix="新建"
                  value={createForm.opcUa ?? defaultOpcUaSettings()}
                  onChange={(opcUa) =>
                    setCreateForm((current) => ({ ...current, opcUa }))
                  }
                />
              ) : null}
              {isBacnetIpProtocol(createForm.protocolType) ? (
                <BacnetIpFields
                  idPrefix="新建"
                  value={createForm.bacnetIp ?? defaultBacnetIpSettings()}
                  onChange={(bacnetIp) =>
                    setCreateForm((current) => ({ ...current, bacnetIp }))
                  }
                />
              ) : null}
              {isSiemensS7Protocol(createForm.protocolType) ? (
                <SiemensS7Fields
                  idPrefix="新建"
                  value={createForm.siemensS7 ?? defaultSiemensS7Settings()}
                  onChange={(siemensS7) =>
                    setCreateForm((current) => ({ ...current, siemensS7 }))
                  }
                />
              ) : null}
              {isOmronFinsProtocol(createForm.protocolType) ? (
                <OmronFinsFields
                  idPrefix="新建"
                  value={createForm.omronFins ?? defaultOmronFinsSettings()}
                  onChange={(omronFins) =>
                    setCreateForm((current) => ({ ...current, omronFins }))
                  }
                />
              ) : null}
              <CircuitBreakerFields
                idPrefix="新建"
                value={createForm.circuitBreaker ?? defaultCircuitBreaker()}
                onChange={(circuitBreaker) =>
                  setCreateForm((current) => ({ ...current, circuitBreaker }))
                }
              />
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
          subtitle="保存并通过校验后自动同步，边端 runtime 平滑重建协议会话"
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
              {isIec104Protocol(form.protocolType) ? (
                <Iec104Fields
                  value={form.iec104}
                  onChange={(iec104) => setForm((current) => ({ ...current, iec104 }))}
                />
              ) : null}
              {isIec101Protocol(form.protocolType) ? (
                <Iec101Fields
                  value={form.iec101}
                  onChange={(iec101) => setForm((current) => ({ ...current, iec101 }))}
                />
              ) : null}
              {isOpcUaProtocol(form.protocolType) ? (
                <OpcUaFields
                  value={form.opcUa}
                  onChange={(opcUa) => setForm((current) => ({ ...current, opcUa }))}
                />
              ) : null}
              {isBacnetIpProtocol(form.protocolType) ? (
                <BacnetIpFields
                  value={form.bacnetIp}
                  onChange={(bacnetIp) =>
                    setForm((current) => ({ ...current, bacnetIp }))
                  }
                />
              ) : null}
              {isSiemensS7Protocol(form.protocolType) ? (
                <SiemensS7Fields
                  value={form.siemensS7}
                  onChange={(siemensS7) =>
                    setForm((current) => ({ ...current, siemensS7 }))
                  }
                />
              ) : null}
              {isOmronFinsProtocol(form.protocolType) ? (
                <OmronFinsFields
                  value={form.omronFins}
                  onChange={(omronFins) =>
                    setForm((current) => ({ ...current, omronFins }))
                  }
                />
              ) : null}
            </div>
          </section>
          <section className="drawer-section">
            <h4>故障保护</h4>
            <p className="section-hint">设备持续异常时暂停南向访问，冷却后自动探测恢复。</p>
            <div className="editor-grid">
              <CircuitBreakerFields
                value={form.circuitBreaker}
                onChange={(circuitBreaker) =>
                  setForm((current) => ({ ...current, circuitBreaker }))
                }
              />
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
  iec101: Iec101ConnectionSettings;
  iec104: Iec104ConnectionSettings;
  opcUa: OpcUaConnectionSettings;
  bacnetIp: BacnetIpConnectionSettings;
  siemensS7: SiemensS7ConnectionSettings;
  omronFins: OmronFinsConnectionSettings;
  circuitBreaker: ProtocolCircuitBreakerConfig;
}

function connectionToEditorForm(connection: ProtocolConnectionResponse): EditorForm {
  return {
    endpoint: connection.endpoint,
    protocolType: connection.protocolType || inferProtocolType(connection.protocol),
    circuitBreaker: connection.circuitBreaker ?? defaultCircuitBreaker(),
    iec101: connection.iec101 ?? defaultIec101Settings(),
    iec104: connection.iec104 ?? defaultIec104Settings(),
    opcUa: connection.opcUa ?? defaultOpcUaSettings(),
    bacnetIp: connection.bacnetIp ?? defaultBacnetIpSettings(),
    siemensS7: connection.siemensS7 ?? defaultSiemensS7Settings(),
    omronFins: connection.omronFins ?? defaultOmronFinsSettings(),
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

function isOpcUaProtocol(protocolType: string | undefined): boolean {
  return protocolType === 'OpcUa';
}

function isIec104Protocol(protocolType: string | undefined): boolean {
  return protocolType === 'Iec104';
}

function isIec101Protocol(protocolType: string | undefined): boolean {
  return protocolType === 'Iec101';
}

function isBacnetIpProtocol(protocolType: string | undefined): boolean {
  return protocolType === 'BacnetIp';
}

function isSiemensS7Protocol(protocolType: string | undefined): boolean {
  return protocolType === 'SiemensS7';
}

function isOmronFinsProtocol(protocolType: string | undefined): boolean {
  return protocolType === 'OmronFins';
}

function endpointForProtocol(protocolType: string, endpoint: string | null | undefined): string {
  const current = endpoint?.trim() ?? '';
  if (isOpcUaProtocol(protocolType)) {
    return current.startsWith('opc.tcp://') ? current : 'opc.tcp://127.0.0.1:4840';
  }
  if (isBacnetIpProtocol(protocolType)) {
    return current && !current.startsWith('/dev/') ? current : '127.0.0.1:47808';
  }
  if (isSiemensS7Protocol(protocolType)) {
    return current && !current.startsWith('/dev/') ? current : '127.0.0.1:102';
  }
  if (isOmronFinsProtocol(protocolType)) {
    return current && !current.startsWith('/dev/') ? current : '127.0.0.1:9600';
  }
  if (isSerialProtocol(protocolType)) {
    return current.startsWith('/dev/') ? current : '/dev/ttyUSB0';
  }
  if (protocolType === 'Iec104') {
    return current && !current.startsWith('/dev/') ? current : '127.0.0.1:2404';
  }
  return current && !current.startsWith('/dev/') ? current : '127.0.0.1:502';
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
  const endpoint = endpointForProtocol(protocolType, form.endpoint);
  return {
    ...form,
    endpoint,
    protocolType,
    serial: isSerialProtocol(protocolType)
      ? serialDefaultsForProtocol(protocolType, form.serial.port || endpoint)
      : form.serial,
    iec101: isIec101Protocol(protocolType) ? form.iec101 : defaultIec101Settings(),
    iec104: isIec104Protocol(protocolType) ? form.iec104 : defaultIec104Settings(),
    opcUa: isOpcUaProtocol(protocolType) ? form.opcUa : defaultOpcUaSettings(),
    bacnetIp: isBacnetIpProtocol(protocolType)
      ? form.bacnetIp
      : defaultBacnetIpSettings(),
    siemensS7: isSiemensS7Protocol(protocolType)
      ? form.siemensS7
      : defaultSiemensS7Settings(),
    omronFins: isOmronFinsProtocol(protocolType)
      ? form.omronFins
      : defaultOmronFinsSettings(),
  };
}

function requestFromEditor(form: EditorForm): SaveProtocolConnectionRequest {
  if (isSerialProtocol(form.protocolType)) {
    return {
      endpoint: form.serial.port.trim() || null,
      protocolType: form.protocolType,
      serial: { ...form.serial, port: form.serial.port.trim() },
      ...(isIec101Protocol(form.protocolType) ? { iec101: form.iec101 } : {}),
      iec104: null,
      opcUa: null,
      circuitBreaker: form.circuitBreaker,
    };
  }
  return {
    endpoint: form.endpoint.trim() || null,
    protocolType: form.protocolType,
    ...(isIec104Protocol(form.protocolType) ? { iec104: form.iec104 } : {}),
    opcUa: isOpcUaProtocol(form.protocolType) ? form.opcUa : null,
    ...(isBacnetIpProtocol(form.protocolType) ? { bacnetIp: form.bacnetIp } : {}),
    ...(isSiemensS7Protocol(form.protocolType)
      ? { siemensS7: form.siemensS7 }
      : {}),
    ...(isOmronFinsProtocol(form.protocolType)
      ? { omronFins: form.omronFins }
      : {}),
    circuitBreaker: form.circuitBreaker,
  };
}

function Iec101Fields({
  idPrefix = '',
  onChange,
  value,
}: {
  idPrefix?: string;
  onChange: (value: Iec101ConnectionSettings) => void;
  value: Iec101ConnectionSettings;
}) {
  return (
    <label className="editor-control">
      <span>CP56Time2a 站端时区偏移（分钟）</span>
      <input
        aria-label={`${idPrefix}IEC 101 CP56 时区偏移`}
        max={840}
        min={-840}
        step={15}
        type="number"
        value={value.cp56TimeZoneOffsetMinutes}
        onChange={(event) =>
          onChange({ cp56TimeZoneOffsetMinutes: Number(event.target.value) })
        }
      />
    </label>
  );
}

function Iec104Fields({
  idPrefix = '',
  onChange,
  value,
}: {
  idPrefix?: string;
  onChange: (value: Iec104ConnectionSettings) => void;
  value: Iec104ConnectionSettings;
}) {
  return (
    <label className="editor-control">
      <span>CP56Time2a 时区偏移（分钟）</span>
      <input
        aria-label={`${idPrefix}CP56 时区偏移`}
        max={840}
        min={-840}
        step={15}
        type="number"
        value={value.cp56TimeZoneOffsetMinutes}
        onChange={(event) =>
          onChange({ cp56TimeZoneOffsetMinutes: Number(event.target.value) })
        }
      />
    </label>
  );
}

function SiemensS7Fields({
  idPrefix = '',
  onChange,
  value,
}: {
  idPrefix?: string;
  onChange: (value: SiemensS7ConnectionSettings) => void;
  value: SiemensS7ConnectionSettings;
}) {
  const label = (text: string) => `${idPrefix}${text}`;
  return (
    <>
      <S7NumberField
        label={label('S7 Rack')}
        title="Rack"
        min={0}
        max={7}
        value={value.rack}
        onChange={(rack) => onChange({ ...value, rack })}
      />
      <S7NumberField
        label={label('S7 Slot')}
        title="Slot"
        min={0}
        max={31}
        value={value.slot}
        onChange={(slot) => onChange({ ...value, slot })}
      />
      <label className="editor-control">
        <span>PDU 大小</span>
        <select
          aria-label={label('S7 PDU 大小')}
          value={value.pduSize}
          onChange={(event) =>
            onChange({
              ...value,
              pduSize: Number(event.target.value) as SiemensS7ConnectionSettings['pduSize'],
            })
          }
        >
          <option value={240}>240 bytes</option>
          <option value={480}>480 bytes</option>
          <option value={960}>960 bytes</option>
        </select>
      </label>
      <S7NumberField
        label={label('S7 连接超时')}
        title="连接超时（ms）"
        min={100}
        max={120_000}
        value={value.connectTimeoutMs}
        onChange={(connectTimeoutMs) => onChange({ ...value, connectTimeoutMs })}
      />
      <S7NumberField
        label={label('S7 请求超时')}
        title="请求超时（ms）"
        min={100}
        max={120_000}
        value={value.requestTimeoutMs}
        onChange={(requestTimeoutMs) => onChange({ ...value, requestTimeoutMs })}
      />
    </>
  );
}

function S7NumberField({
  label,
  max,
  min,
  onChange,
  title,
  value,
}: {
  label: string;
  max: number;
  min: number;
  onChange: (value: number) => void;
  title: string;
  value: number;
}) {
  return (
    <label className="editor-control">
      <span>{title}</span>
      <input
        aria-label={label}
        max={max}
        min={min}
        type="number"
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
    </label>
  );
}

function OmronFinsFields({
  idPrefix = '',
  onChange,
  value,
}: {
  idPrefix?: string;
  onChange: (value: OmronFinsConnectionSettings) => void;
  value: OmronFinsConnectionSettings;
}) {
  const label = (text: string) => `${idPrefix}${text}`;
  const numberField = (
    title: string,
    key: keyof Omit<OmronFinsConnectionSettings, 'transport' | 'wordOrder'>,
    min: number,
    max: number,
  ) => (
    <label className="editor-control" key={key}>
      <span>{title}</span>
      <input
        aria-label={label(`FINS ${title}`)}
        min={min}
        max={max}
        type="number"
        value={value[key]}
        onChange={(event) =>
          onChange({ ...value, [key]: Number(event.target.value) })
        }
      />
    </label>
  );
  return (
    <>
      <label className="editor-control">
        <span>传输方式</span>
        <select
          aria-label={label('FINS 传输方式')}
          value={value.transport}
          onChange={(event) => {
            const transport = event.target.value as OmronFinsConnectionSettings['transport'];
            onChange({
              ...value,
              transport,
              sourceNode: transport === 'udp' && value.sourceNode === 0 ? 1 : value.sourceNode,
            });
          }}
        >
          <option value="udp">FINS/UDP</option>
          <option value="tcp">FINS/TCP（节点握手）</option>
        </select>
      </label>
      {numberField('源网络号', 'sourceNetwork', 0, 127)}
      {numberField('源节点号', 'sourceNode', value.transport === 'tcp' ? 0 : 1, 254)}
      {numberField('源单元号', 'sourceUnit', 0, 255)}
      {numberField('目标网络号', 'destinationNetwork', 0, 127)}
      {numberField('目标节点号', 'destinationNode', 0, 254)}
      {numberField('目标单元号', 'destinationUnit', 0, 255)}
      {numberField('请求超时（ms）', 'timeoutMs', 100, 120_000)}
      <label className="editor-control">
        <span>双字字序</span>
        <select
          aria-label={label('FINS 双字字序')}
          value={value.wordOrder}
          onChange={(event) =>
            onChange({
              ...value,
              wordOrder: event.target.value as OmronFinsConnectionSettings['wordOrder'],
            })
          }
        >
          <option value="low_word_first">低字在前（Omron 默认）</option>
          <option value="high_word_first">高字在前</option>
        </select>
      </label>
    </>
  );
}

function BacnetIpFields({
  idPrefix = '',
  onChange,
  value,
}: {
  idPrefix?: string;
  onChange: (value: BacnetIpConnectionSettings) => void;
  value: BacnetIpConnectionSettings;
}) {
  const label = (text: string) => `${idPrefix}${text}`;
  return (
    <>
      <label className="editor-control">
        <span>绑定网卡地址</span>
        <input
          aria-label={label('BACnet 绑定网卡地址')}
          placeholder="0.0.0.0"
          value={value.bindAddress}
          onChange={(event) => onChange({ ...value, bindAddress: event.target.value })}
        />
      </label>
      <label className="editor-control">
        <span>本地 UDP 端口</span>
        <input
          aria-label={label('BACnet 本地 UDP 端口')}
          max={65535}
          min={0}
          type="number"
          value={value.localPort}
          onChange={(event) => onChange({ ...value, localPort: Number(event.target.value) })}
        />
      </label>
      <label className="editor-control">
        <span>广播地址</span>
        <input
          aria-label={label('BACnet 广播地址')}
          placeholder="255.255.255.255"
          value={value.broadcastAddress}
          onChange={(event) =>
            onChange({ ...value, broadcastAddress: event.target.value })
          }
        />
      </label>
      <label className="editor-control">
        <span>最大 APDU</span>
        <select
          aria-label={label('BACnet 最大 APDU')}
          value={value.maxApduLength}
          onChange={(event) =>
            onChange({
              ...value,
              maxApduLength: Number(event.target.value) as BacnetIpConnectionSettings['maxApduLength'],
            })
          }
        >
          {[50, 128, 206, 480, 1024, 1476].map((size) => (
            <option key={size} value={size}>{size} bytes</option>
          ))}
        </select>
      </label>
      <BacnetNumberField
        label={label('BACnet APDU 超时')}
        title="APDU 超时（ms）"
        min={100}
        max={120_000}
        value={value.apduTimeoutMs}
        onChange={(apduTimeoutMs) => onChange({ ...value, apduTimeoutMs })}
      />
      <BacnetNumberField
        label={label('BACnet APDU 重试')}
        title="APDU 重试次数"
        min={0}
        max={10}
        value={value.apduRetries}
        onChange={(apduRetries) => onChange({ ...value, apduRetries })}
      />
      <BacnetNumberField
        label={label('BACnet 发现超时')}
        title="设备发现超时（ms）"
        min={100}
        max={30_000}
        value={value.discoveryTimeoutMs}
        onChange={(discoveryTimeoutMs) => onChange({ ...value, discoveryTimeoutMs })}
      />
      <label className="mqtt-toggle-field editor-control">
        <span>
          <strong>BBMD 外部设备</strong>
          <small>跨子网时向 BBMD 注册并自动续租</small>
        </span>
        <input
          aria-label={label('BACnet BBMD 外部设备')}
          checked={value.foreignDevice != null}
          type="checkbox"
          onChange={(event) =>
            onChange({
              ...value,
              foreignDevice: event.target.checked
                ? { bbmdAddress: '127.0.0.1:47808', ttlSeconds: 120 }
                : null,
            })
          }
        />
        <i aria-hidden="true" />
      </label>
      {value.foreignDevice ? (
        <>
          <label className="editor-control">
            <span>BBMD 地址</span>
            <input
              aria-label={label('BACnet BBMD 地址')}
              placeholder="10.12.0.10:47808"
              value={value.foreignDevice.bbmdAddress}
              onChange={(event) =>
                onChange({
                  ...value,
                  foreignDevice: {
                    ...value.foreignDevice!,
                    bbmdAddress: event.target.value,
                  },
                })
              }
            />
          </label>
          <BacnetNumberField
            label={label('BACnet BBMD TTL')}
            title="注册有效期（秒）"
            min={30}
            max={65_535}
            value={value.foreignDevice.ttlSeconds}
            onChange={(ttlSeconds) =>
              onChange({
                ...value,
                foreignDevice: { ...value.foreignDevice!, ttlSeconds },
              })
            }
          />
        </>
      ) : null}
      <label className="mqtt-toggle-field editor-control">
        <span>
          <strong>COV 变化订阅</strong>
          <small>设备变化时主动上送，异常时自动恢复轮询</small>
        </span>
        <input
          aria-label={label('BACnet COV 变化订阅')}
          checked={value.cov != null}
          type="checkbox"
          onChange={(event) =>
            onChange({
              ...value,
              cov: event.target.checked
                ? {
                    lifetimeSeconds: 300,
                    confirmedNotifications: false,
                    fallbackPollIntervalMs: 60_000,
                  }
                : null,
            })
          }
        />
        <i aria-hidden="true" />
      </label>
      {value.cov ? (
        <>
          <BacnetNumberField
            label={label('BACnet COV 租期')}
            title="订阅租期（秒）"
            min={60}
            max={86_400}
            value={value.cov.lifetimeSeconds}
            onChange={(lifetimeSeconds) =>
              onChange({
                ...value,
                cov: { ...value.cov!, lifetimeSeconds },
              })
            }
          />
          <BacnetNumberField
            label={label('BACnet COV 降级轮询')}
            title="降级轮询间隔（ms）"
            min={1_000}
            max={3_600_000}
            value={value.cov.fallbackPollIntervalMs}
            onChange={(fallbackPollIntervalMs) =>
              onChange({
                ...value,
                cov: { ...value.cov!, fallbackPollIntervalMs },
              })
            }
          />
          <label className="mqtt-toggle-field editor-control">
            <span>
              <strong>确认型通知</strong>
              <small>要求 Runtime 对每条 COV 通知应答</small>
            </span>
            <input
              aria-label={label('BACnet COV 确认型通知')}
              checked={value.cov.confirmedNotifications}
              type="checkbox"
              onChange={(event) =>
                onChange({
                  ...value,
                  cov: {
                    ...value.cov!,
                    confirmedNotifications: event.target.checked,
                  },
                })
              }
            />
            <i aria-hidden="true" />
          </label>
        </>
      ) : null}
    </>
  );
}

function BacnetNumberField({
  label,
  max,
  min,
  onChange,
  title,
  value,
}: {
  label: string;
  max: number;
  min: number;
  onChange: (value: number) => void;
  title: string;
  value: number;
}) {
  return (
    <label className="editor-control">
      <span>{title}</span>
      <input
        aria-label={label}
        max={max}
        min={min}
        type="number"
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
    </label>
  );
}

function OpcUaFields({
  idPrefix = '',
  onChange,
  value,
}: {
  idPrefix?: string;
  onChange: (value: OpcUaConnectionSettings) => void;
  value: OpcUaConnectionSettings;
}) {
  const label = (text: string) => `${idPrefix}${text}`;
  return (
    <>
      <label className="editor-control">
        <span>安全策略</span>
        <select
          aria-label={label('OPC UA 安全策略')}
          value={value.securityPolicy}
          onChange={(event) => {
            const securityPolicy = event.target
              .value as OpcUaConnectionSettings['securityPolicy'];
            onChange({
              ...value,
              securityPolicy,
              messageSecurityMode:
                securityPolicy === 'none'
                  ? 'none'
                  : value.messageSecurityMode === 'none'
                    ? 'sign_and_encrypt'
                    : value.messageSecurityMode,
            });
          }}
        >
          <option value="none">None</option>
          <option value="basic256_sha256">Basic256Sha256</option>
          <option value="aes128_sha256_rsa_oaep">Aes128-Sha256-RsaOaep</option>
          <option value="aes256_sha256_rsa_pss">Aes256-Sha256-RsaPss</option>
        </select>
      </label>
      <label className="editor-control">
        <span>消息安全模式</span>
        <select
          aria-label={label('OPC UA 消息安全模式')}
          value={value.messageSecurityMode}
          onChange={(event) =>
            onChange({
              ...value,
              messageSecurityMode: event.target
                .value as OpcUaConnectionSettings['messageSecurityMode'],
            })
          }
        >
          <option value="none" disabled={value.securityPolicy !== 'none'}>
            None
          </option>
          <option value="sign" disabled={value.securityPolicy === 'none'}>
            Sign
          </option>
          <option value="sign_and_encrypt" disabled={value.securityPolicy === 'none'}>
            Sign &amp; Encrypt
          </option>
        </select>
      </label>
      <label className="editor-control">
        <span>身份认证</span>
        <select
          aria-label={label('OPC UA 身份认证')}
          value={value.authMode}
          onChange={(event) => {
            const authMode = event.target.value as OpcUaConnectionSettings['authMode'];
            onChange({
              ...value,
              authMode,
              username: authMode === 'username' ? value.username : null,
              passwordEnv: authMode === 'username' ? value.passwordEnv : null,
              userCertificatePath:
                authMode === 'x509' ? value.userCertificatePath : null,
              userPrivateKeyPath:
                authMode === 'x509' ? value.userPrivateKeyPath : null,
            });
          }}
        >
          <option value="anonymous">匿名</option>
          <option value="username">用户名 / 密码</option>
          <option value="x509">X.509 用户证书</option>
        </select>
      </label>
      {value.authMode === 'username' ? (
        <>
          <label className="editor-control">
            <span>用户名</span>
            <input
              aria-label={label('OPC UA 用户名')}
              value={value.username ?? ''}
              onChange={(event) => onChange({ ...value, username: event.target.value })}
            />
          </label>
          <label className="editor-control">
            <span>密码环境变量</span>
            <input
              aria-label={label('OPC UA 密码环境变量')}
              placeholder="VELAEDGE_OPCUA_PASSWORD"
              value={value.passwordEnv ?? ''}
              onChange={(event) => onChange({ ...value, passwordEnv: event.target.value })}
            />
          </label>
        </>
      ) : null}
      {value.authMode === 'x509' ? (
        <>
          <label className="editor-control">
            <span>用户证书路径</span>
            <input
              aria-label={label('OPC UA 用户证书路径')}
              value={value.userCertificatePath ?? ''}
              onChange={(event) =>
                onChange({ ...value, userCertificatePath: event.target.value })
              }
            />
          </label>
          <label className="editor-control">
            <span>用户私钥路径</span>
            <input
              aria-label={label('OPC UA 用户私钥路径')}
              value={value.userPrivateKeyPath ?? ''}
              onChange={(event) =>
                onChange({ ...value, userPrivateKeyPath: event.target.value })
              }
            />
          </label>
        </>
      ) : null}
      <label className="editor-control">
        <span>PKI 目录</span>
        <input
          aria-label={label('OPC UA PKI 目录')}
          value={value.pkiDir}
          onChange={(event) => onChange({ ...value, pkiDir: event.target.value })}
        />
      </label>
      <label className="mqtt-toggle-field editor-control">
        <span>
          <strong>信任未知证书</strong>
          <small>首次接入时自动加入信任</small>
        </span>
        <input
          aria-label={label('OPC UA 信任未知证书')}
          checked={value.trustServerCerts}
          onChange={(event) =>
            onChange({ ...value, trustServerCerts: event.target.checked })
          }
          type="checkbox"
        />
        <i aria-hidden="true" />
      </label>
      <label className="mqtt-toggle-field editor-control">
        <span>
          <strong>校验服务端证书</strong>
          <small>校验证书有效期、主机和用途</small>
        </span>
        <input
          aria-label={label('OPC UA 校验服务端证书')}
          checked={value.verifyServerCerts}
          onChange={(event) =>
            onChange({ ...value, verifyServerCerts: event.target.checked })
          }
          type="checkbox"
        />
        <i aria-hidden="true" />
      </label>
      <OpcUaNumberField
        label={label('OPC UA 连接超时')}
        title="连接超时（ms）"
        min={100}
        max={120_000}
        value={value.connectTimeoutMs}
        onChange={(connectTimeoutMs) => onChange({ ...value, connectTimeoutMs })}
      />
      <OpcUaNumberField
        label={label('OPC UA 请求超时')}
        title="请求超时（ms）"
        min={100}
        max={120_000}
        value={value.requestTimeoutMs}
        onChange={(requestTimeoutMs) => onChange({ ...value, requestTimeoutMs })}
      />
      <OpcUaNumberField
        label={label('OPC UA 会话超时')}
        title="会话超时（ms）"
        min={1_000}
        max={3_600_000}
        value={value.sessionTimeoutMs}
        onChange={(sessionTimeoutMs) => onChange({ ...value, sessionTimeoutMs })}
      />
      <OpcUaNumberField
        label={label('OPC UA 重试次数')}
        title="会话重试次数"
        min={0}
        max={100}
        value={value.sessionRetryLimit}
        onChange={(sessionRetryLimit) => onChange({ ...value, sessionRetryLimit })}
      />
    </>
  );
}

function OpcUaNumberField({
  label,
  max,
  min,
  onChange,
  title,
  value,
}: {
  label: string;
  max: number;
  min: number;
  onChange: (value: number) => void;
  title: string;
  value: number;
}) {
  return (
    <label className="editor-control">
      <span>{title}</span>
      <input
        aria-label={label}
        max={max}
        min={min}
        onChange={(event) => onChange(Number(event.target.value))}
        type="number"
        value={value}
      />
    </label>
  );
}

function CircuitBreakerFields({
  idPrefix = '',
  onChange,
  value,
}: {
  idPrefix?: string;
  onChange: (value: ProtocolCircuitBreakerConfig) => void;
  value: ProtocolCircuitBreakerConfig;
}) {
  const label = (text: string) => `${idPrefix}${text}`;
  return (
    <>
      <label className="mqtt-toggle-field editor-control">
        <span>
          <strong>自动熔断</strong>
          <small>连续失败后暂停访问设备</small>
        </span>
        <input
          aria-label={label('自动熔断')}
          checked={value.enabled}
          onChange={(event) => onChange({ ...value, enabled: event.target.checked })}
          type="checkbox"
        />
        <i aria-hidden="true" />
      </label>
      <label className="editor-control">
        <span>连续失败阈值</span>
        <input
          aria-label={label('连续失败阈值')}
          disabled={!value.enabled}
          max={100}
          min={1}
          onChange={(event) =>
            onChange({ ...value, failureThreshold: Number(event.target.value) })
          }
          type="number"
          value={value.failureThreshold}
        />
      </label>
      <label className="editor-control">
        <span>冷却时间（秒）</span>
        <input
          aria-label={label('冷却时间')}
          disabled={!value.enabled}
          max={3600}
          min={1}
          onChange={(event) =>
            onChange({ ...value, openDurationMs: Number(event.target.value) * 1_000 })
          }
          type="number"
          value={value.openDurationMs / 1_000}
        />
      </label>
      <label className="editor-control">
        <span>恢复探测成功次数</span>
        <input
          aria-label={label('恢复探测成功次数')}
          disabled={!value.enabled}
          max={10}
          min={1}
          onChange={(event) =>
            onChange({
              ...value,
              halfOpenSuccessThreshold: Number(event.target.value),
            })
          }
          type="number"
          value={value.halfOpenSuccessThreshold}
        />
      </label>
    </>
  );
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
  if (normalized.includes('bacnet')) {
    return 'BacnetIp';
  }
  if (normalized.includes('rtu') || normalized.includes('rs485')) {
    return 'ModbusRtu';
  }
  if (normalized.includes('s7') || normalized.includes('siemens')) {
    return 'SiemensS7';
  }
  if (normalized.includes('omron') || normalized.includes('fins')) {
    return 'OmronFins';
  }
  if (normalized.includes('dlt') || normalized.includes('645')) {
    return 'Dlt645';
  }
  if (normalized.includes('104')) {
    return 'Iec104';
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
