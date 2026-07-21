import { useEffect, useState, type Dispatch, type SetStateAction } from 'react';
import { Plus, Trash2, X } from 'lucide-react';

import type {
  CreateDeviceModelRequest,
  DeviceModelResponse,
  SaveDeviceModelRequest,
} from '../api/types';
import { Drawer } from '../components/Drawer';
import { Modal } from '../components/Modal';
import { PaginationBar } from '../components/PaginationBar';
import { displayError } from '../utils/errors';
import './PointMappingsPage.css';

const fallbackDeviceModels: DeviceModelResponse[] = [
  {
    deviceType: 'pump',
    version: 'v1',
    commandCount: 1,
    eventCount: 1,
    telemetry: [
      {
        telemetryId: 'pressure',
        name: 'pressure',
        valueType: 'float32',
        unit: 'MPa',
        range: '0-20',
        description: '泵出口压力',
      },
      {
        telemetryId: 'running',
        name: 'running',
        valueType: 'bool',
        unit: '-',
        range: '-',
        description: '设备运行布尔量',
      },
    ],
  },
];

export function DeviceModelsPage({
  deviceModels = fallbackDeviceModels,
  onCreateDeviceModel,
  onDeleteDeviceModel,
  onSaveDeviceModel,
}: {
  deviceModels?: DeviceModelResponse[];
  onCreateDeviceModel?: (
    request: CreateDeviceModelRequest,
  ) => Promise<DeviceModelResponse> | DeviceModelResponse;
  onDeleteDeviceModel?: (deviceType: string) => Promise<void> | void;
  onSaveDeviceModel?: (
    deviceType: string,
    request: SaveDeviceModelRequest,
  ) => Promise<DeviceModelResponse> | DeviceModelResponse;
}) {
  const [selectedDeviceType, setSelectedDeviceType] = useState(
    () => deviceModels[0]?.deviceType ?? fallbackDeviceModels[0].deviceType,
  );
  const [page, setPage] = useState(1);
  const activeModel =
    deviceModels.find((model) => model.deviceType === selectedDeviceType) ??
    deviceModels[0] ??
    fallbackDeviceModels[0];
  const [toolbarMessage, setToolbarMessage] = useState('');
  const [createState, setCreateState] = useState<'idle' | 'creating'>('idle');
  const [saveState, setSaveState] = useState<'idle' | 'saving' | 'saved' | 'error'>(
    'idle',
  );
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editDialogOpen, setEditDialogOpen] = useState(false);
  const [createForm, setCreateForm] = useState<CreateDeviceModelRequest>(() =>
    emptyCreateForm(),
  );
  const [editForm, setEditForm] = useState<SaveDeviceModelRequest>(() =>
    modelToSaveForm(activeModel),
  );
  const pageSize = 10;
  const totalPages = Math.max(1, Math.ceil(deviceModels.length / pageSize));
  const currentPage = Math.min(page, totalPages);
  const visibleDeviceModels = deviceModels.slice(
    (currentPage - 1) * pageSize,
    currentPage * pageSize,
  );

  useEffect(() => {
    if (
      deviceModels.length > 0 &&
      !deviceModels.some((model) => model.deviceType === selectedDeviceType)
    ) {
      setSelectedDeviceType(deviceModels[0].deviceType);
    }
  }, [deviceModels, selectedDeviceType]);

  useEffect(() => {
    setEditForm(modelToSaveForm(activeModel));
    setSaveState((current) => (current === 'saving' || current === 'saved' ? current : 'idle'));
  }, [activeModel]);

  useEffect(() => {
    setPage(1);
  }, [deviceModels]);

  const handleCreateDeviceModel = async () => {
    setCreateState('creating');
    setToolbarMessage('');

    try {
      const created = await onCreateDeviceModel?.(createForm);
      setToolbarMessage(
        created
          ? `已创建设备模型 ${created.deviceType}@${created.version}`
          : '已创建设备模型',
      );
      if (created) {
        setSelectedDeviceType(created.deviceType);
      }
      setCreateForm(emptyCreateForm());
      setDialogOpen(false);
    } catch (error) {
      setToolbarMessage(`创建设备模型失败：${displayError(error)}`);
    } finally {
      setCreateState('idle');
    }
  };

  const handleSaveDeviceModel = async () => {
    setSaveState('saving');

    try {
      const saved = await onSaveDeviceModel?.(activeModel.deviceType, normalizeSaveForm(editForm));
      if (saved) {
        setSelectedDeviceType(saved.deviceType);
      }
      setSaveState('saved');
    } catch (error) {
      setSaveState('error');
      setToolbarMessage(`保存设备模型失败：${displayError(error)}`);
    }
  };

  const handleDeleteDeviceModel = async (deviceType: string) => {
    setToolbarMessage('');
    try {
      await onDeleteDeviceModel?.(deviceType);
      setToolbarMessage(`已删除设备模型 ${deviceType}`);
    } catch (error) {
      setToolbarMessage(`删除设备模型失败：${displayError(error, '请先解除设备实例引用')}`);
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>语义设备模型</h2>
          <p>设备语义与遥测字段定义。</p>
        </div>
        <div className="toolbar">
          {toolbarMessage ? (
            <span className="toolbar-status" role="status">
              {toolbarMessage}
            </span>
          ) : null}
          <button
            className="primary-button"
            onClick={() => setDialogOpen(true)}
            type="button"
          >
            <Plus size={15} aria-hidden="true" />
            新建设备模型
          </button>
        </div>
      </section>

      {dialogOpen ? (
        <DeviceModelDialog
          actionState={createState}
          form={createForm}
          onClose={() => setDialogOpen(false)}
          onSubmit={() => {
            void handleCreateDeviceModel();
          }}
          setForm={setCreateForm}
        />
      ) : null}

      <div className="point-config-layout">
        <section className="panel point-table-panel">
          <div className="panel-header">
            <h3>设备模型清单</h3>
            <span>{deviceModels.length} 个模型</span>
          </div>
          <div className="table-wrap">
            <table className="ops-table">
              <thead>
                <tr>
                  <th>设备类型</th>
                  <th>版本</th>
                  <th>遥测</th>
                  <th>命令 / 事件</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                {visibleDeviceModels.map((model) => (
                  <tr key={`${model.deviceType}:${model.version}`}>
                    <td>
                      <button
                        aria-label={`选择设备模型 ${model.deviceType}`}
                        aria-pressed={model.deviceType === activeModel.deviceType}
                        className="point-id-button"
                        onClick={() => {
                          setSelectedDeviceType(model.deviceType);
                          setEditDialogOpen(true);
                        }}
                        type="button"
                      >
                        {model.deviceType}
                      </button>
                    </td>
                    <td>{model.version}</td>
                    <td>{model.telemetry.map((telemetry) => telemetry.telemetryId).join(', ')}</td>
                    <td>
                      {model.commandCount} / {model.eventCount}
                    </td>
                    <td>
                      <div className="row-actions">
                        <button
                          aria-label={`删除设备模型 ${model.deviceType}`}
                          className="danger-button compact"
                          onClick={() => {
                            void handleDeleteDeviceModel(model.deviceType);
                          }}
                          type="button"
                        >
                          <Trash2 size={14} aria-hidden="true" />
                          删除
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {deviceModels.length > pageSize ? (
            <PaginationBar
              ariaLabel="设备模型分页"
              currentPage={currentPage}
              onNext={() => setPage((value) => Math.min(totalPages, value + 1))}
              onPrevious={() => setPage((value) => Math.max(1, value - 1))}
              totalPages={totalPages}
            />
          ) : null}
        </section>

        {editDialogOpen ? (
        <Drawer
          onClose={() => setEditDialogOpen(false)}
          subtitle="模型保存后进入待发布配置，发布后边端按语义点位匹配协议地址"
          title={`编辑设备模型 ${activeModel.deviceType}`}
          footer={
            <>
              <span className={`editor-status ${saveState}`} role="status">
                {saveStatusText(saveState)}
              </span>
              <button
                className="secondary-button"
                onClick={() => {
                  setEditForm(modelToSaveForm(activeModel));
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
                onClick={() => {
                  void handleSaveDeviceModel();
                }}
                type="button"
              >
                {saveState === 'saving' ? '保存中' : '保存'}
              </button>
            </>
          }
        >
          <section className="drawer-section">
            <h4>模型信息</h4>
            <div className="editor-grid">
              <div className="editor-field">
                <span>设备类型</span>
                <strong>{activeModel.deviceType}</strong>
              </div>
              <label className="editor-control">
                <span>模型版本</span>
                <input
                  aria-label="模型版本"
                  value={editForm.version}
                  onChange={(event) =>
                    setEditForm((current) => ({
                      ...current,
                      version: event.target.value,
                    }))
                  }
                />
              </label>
            </div>
          </section>
          <section className="drawer-section">
            <h4>遥测定义</h4>
            <div className="editor-grid">
              <label className="editor-control">
                <span>遥测 ID</span>
                <input
                  aria-label="遥测 ID"
                  value={editForm.telemetry[0]?.telemetryId ?? ''}
                  onChange={(event) =>
                    updateTelemetry(setEditForm, 'telemetryId', event.target.value)
                  }
                />
              </label>
              <label className="editor-control">
                <span>数据类型</span>
                <select
                  aria-label="数据类型"
                  value={editForm.telemetry[0]?.valueType ?? 'float32'}
                  onChange={(event) =>
                    updateTelemetry(setEditForm, 'valueType', event.target.value)
                  }
                >
                  <option value="float32">float32</option>
                  <option value="int64">int64</option>
                  <option value="bool">bool</option>
                  <option value="string">string</option>
                </select>
              </label>
              <label className="editor-control">
                <span>单位</span>
                <input
                  aria-label="单位"
                  value={editForm.telemetry[0]?.unit ?? ''}
                  onChange={(event) => updateTelemetry(setEditForm, 'unit', event.target.value)}
                />
              </label>
              <label className="editor-control">
                <span>范围</span>
                <input
                  aria-label="范围"
                  value={editForm.telemetry[0]?.range ?? ''}
                  onChange={(event) => updateTelemetry(setEditForm, 'range', event.target.value)}
                />
              </label>
              <label className="editor-control">
                <span>说明</span>
                <input
                  aria-label="说明"
                  value={editForm.telemetry[0]?.description ?? ''}
                  onChange={(event) =>
                    updateTelemetry(setEditForm, 'description', event.target.value)
                  }
                />
              </label>
            </div>
          </section>
          <section className="drawer-section">
            <h4>
              {activeModel.deviceType}@{activeModel.version} 遥测定义
            </h4>
            <div className="table-wrap">
              <table className="ops-table">
                <thead>
                  <tr>
                    <th>Telemetry ID</th>
                    <th>名称</th>
                    <th>数据类型</th>
                    <th>单位</th>
                    <th>范围</th>
                  </tr>
                </thead>
                <tbody>
                  {activeModel.telemetry.map((telemetry) => (
                    <tr key={telemetry.telemetryId}>
                      <td>{telemetry.telemetryId}</td>
                      <td>{telemetry.name}</td>
                      <td>{telemetry.valueType}</td>
                      <td>{telemetry.unit}</td>
                      <td>{telemetry.range}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        </Drawer>
        ) : null}
      </div>
    </div>
  );
}

function DeviceModelDialog({
  actionState,
  form,
  onClose,
  onSubmit,
  setForm,
}: {
  actionState: 'idle' | 'creating';
  form: CreateDeviceModelRequest;
  onClose: () => void;
  onSubmit: () => void;
  setForm: Dispatch<SetStateAction<CreateDeviceModelRequest>>;
}) {
  return (
    <Modal onClose={onClose}>
      <form
        aria-labelledby="device-model-dialog-title"
        className="modal-panel"
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit();
        }}
        role="dialog"
      >
        <div className="modal-header">
          <h3 id="device-model-dialog-title">新建设备模型</h3>
          <button aria-label="关闭" className="icon-button" onClick={onClose} type="button">
            <X size={16} aria-hidden="true" />
          </button>
        </div>
        <div className="form-grid">
          <label>
            <span>设备类型</span>
            <input
              aria-label="设备类型"
              required
              value={form.deviceType}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  deviceType: event.target.value,
                }))
              }
            />
          </label>
          <label>
            <span>模型版本</span>
            <input
              aria-label="模型版本"
              required
              value={form.version}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  version: event.target.value,
                }))
              }
            />
          </label>
          <TelemetryFormFields form={form} setForm={setForm} />
        </div>
        <div className="modal-actions">
          <button className="secondary-button" onClick={onClose} type="button">
            取消
          </button>
          <button className="primary-button" disabled={actionState === 'creating'} type="submit">
            {actionState === 'creating' ? '保存中' : '保存设备模型'}
          </button>
        </div>
      </form>
    </Modal>
  );
}

function TelemetryFormFields({
  form,
  setForm,
}: {
  form: CreateDeviceModelRequest;
  setForm: Dispatch<SetStateAction<CreateDeviceModelRequest>>;
}) {
  const update = (
    field: keyof CreateDeviceModelRequest['telemetry'][number],
    value: string,
  ) => {
    setForm((current) => ({
      ...current,
      telemetry: [
        {
          ...current.telemetry[0],
          [field]: value,
        },
      ],
    }));
  };

  return (
    <>
      <label>
        <span>遥测 ID</span>
        <input
          aria-label="遥测 ID"
          required
          value={form.telemetry[0].telemetryId}
          onChange={(event) => update('telemetryId', event.target.value)}
        />
      </label>
      <label>
        <span>数据类型</span>
        <select
          aria-label="数据类型"
          value={form.telemetry[0].valueType}
          onChange={(event) => update('valueType', event.target.value)}
        >
          <option value="float32">float32</option>
          <option value="int64">int64</option>
          <option value="bool">bool</option>
          <option value="string">string</option>
        </select>
      </label>
      <label>
        <span>单位</span>
        <input
          aria-label="单位"
          value={form.telemetry[0].unit}
          onChange={(event) => update('unit', event.target.value)}
        />
      </label>
      <label>
        <span>范围</span>
        <input
          aria-label="范围"
          placeholder="0-100"
          value={form.telemetry[0].range}
          onChange={(event) => update('range', event.target.value)}
        />
      </label>
      <label className="form-wide">
        <span>说明</span>
        <input
          aria-label="说明"
          value={form.telemetry[0].description}
          onChange={(event) => update('description', event.target.value)}
        />
      </label>
    </>
  );
}

function emptyCreateForm(): CreateDeviceModelRequest {
  return {
    deviceType: '',
    version: 'v1',
    telemetry: [
      {
        description: '',
        range: '',
        telemetryId: '',
        unit: '',
        valueType: 'float32',
      },
    ],
  };
}

function modelToSaveForm(model: DeviceModelResponse): SaveDeviceModelRequest {
  const telemetry = model.telemetry[0] ?? {
    description: '',
    range: '',
    telemetryId: '',
    unit: '',
    valueType: 'float32',
  };

  return {
    version: model.version,
    telemetry: [
      {
        description: telemetry.description === '-' ? '' : telemetry.description,
        range: telemetry.range === '-' ? '' : telemetry.range,
        telemetryId: telemetry.telemetryId,
        unit: telemetry.unit === '-' ? '' : telemetry.unit,
        valueType: telemetry.valueType,
      },
    ],
  };
}

function normalizeSaveForm(form: SaveDeviceModelRequest): SaveDeviceModelRequest {
  return {
    version: form.version.trim(),
    telemetry: form.telemetry.map((telemetry) => ({
      description: telemetry.description?.trim() ?? '',
      range: telemetry.range?.trim() ?? '',
      telemetryId: telemetry.telemetryId.trim(),
      unit: telemetry.unit?.trim() ?? '',
      valueType: telemetry.valueType,
    })),
  };
}

function updateTelemetry(
  setForm: Dispatch<SetStateAction<SaveDeviceModelRequest>>,
  field: keyof SaveDeviceModelRequest['telemetry'][number],
  value: string,
) {
  setForm((current) => ({
    ...current,
    telemetry: [
      {
        ...current.telemetry[0],
        [field]: value,
      },
    ],
  }));
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
