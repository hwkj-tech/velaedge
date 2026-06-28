import { useState } from 'react';
import { Plus, X } from 'lucide-react';

import type { CreateDeviceModelRequest, DeviceModelResponse } from '../api/types';

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
}: {
  deviceModels?: DeviceModelResponse[];
  onCreateDeviceModel?: (
    request: CreateDeviceModelRequest,
  ) => Promise<DeviceModelResponse> | DeviceModelResponse;
}) {
  const activeModel = deviceModels[0] ?? fallbackDeviceModels[0];
  const [toolbarMessage, setToolbarMessage] = useState('');
  const [actionState, setActionState] = useState<'idle' | 'creating'>('idle');
  const [dialogOpen, setDialogOpen] = useState(false);
  const [form, setForm] = useState<CreateDeviceModelRequest>({
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
  });

  const updateTelemetry = (
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

  const handleCreateDeviceModel = async () => {
    setActionState('creating');
    setToolbarMessage('');

    try {
      const created = await onCreateDeviceModel?.(form);
      setToolbarMessage(
        created
          ? `已创建设备模型 ${created.deviceType}@${created.version}`
          : '已创建设备模型',
      );
      setDialogOpen(false);
    } catch {
      setToolbarMessage('创建设备模型失败');
    } finally {
      setActionState('idle');
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>语义设备模型</h2>
          <p>
            先定义设备语义，再在点位配置中绑定具体 Modbus RTU、DL/T645、IEC 101 或模拟地址。
          </p>
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
        <div className="modal-backdrop">
          <form
            aria-labelledby="device-model-dialog-title"
            className="modal-panel"
            onSubmit={(event) => {
              event.preventDefault();
              void handleCreateDeviceModel();
            }}
            role="dialog"
          >
            <div className="modal-header">
              <h3 id="device-model-dialog-title">新建设备模型</h3>
              <button
                aria-label="关闭"
                className="icon-button"
                onClick={() => setDialogOpen(false)}
                type="button"
              >
                <X size={16} aria-hidden="true" />
              </button>
            </div>
            <div className="form-grid">
              <label>
                <span>设备类型</span>
                <input
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
              <label>
                <span>遥测 ID</span>
                <input
                  required
                  value={form.telemetry[0].telemetryId}
                  onChange={(event) => updateTelemetry('telemetryId', event.target.value)}
                />
              </label>
              <label>
                <span>数据类型</span>
                <select
                  value={form.telemetry[0].valueType}
                  onChange={(event) => updateTelemetry('valueType', event.target.value)}
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
                  value={form.telemetry[0].unit}
                  onChange={(event) => updateTelemetry('unit', event.target.value)}
                />
              </label>
              <label>
                <span>范围</span>
                <input
                  placeholder="0-100"
                  value={form.telemetry[0].range}
                  onChange={(event) => updateTelemetry('range', event.target.value)}
                />
              </label>
              <label className="form-wide">
                <span>说明</span>
                <input
                  value={form.telemetry[0].description}
                  onChange={(event) => updateTelemetry('description', event.target.value)}
                />
              </label>
            </div>
            <div className="modal-actions">
              <button
                className="secondary-button"
                onClick={() => setDialogOpen(false)}
                type="button"
              >
                取消
              </button>
              <button
                className="primary-button"
                disabled={actionState === 'creating'}
                type="submit"
              >
                {actionState === 'creating' ? '保存中' : '保存设备模型'}
              </button>
            </div>
          </form>
        </div>
      ) : null}

      <section className="panel">
        <div className="panel-header">
          <h3>
            {activeModel.deviceType}@{activeModel.version} 遥测定义
          </h3>
          <span>
            命令 {activeModel.commandCount} 个 / 事件 {activeModel.eventCount} 个
          </span>
        </div>
        <div className="table-wrap">
          <table className="ops-table">
            <thead>
              <tr>
                <th>Telemetry ID</th>
                <th>名称</th>
                <th>数据类型</th>
                <th>单位</th>
                <th>范围</th>
                <th>说明</th>
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
                  <td>{telemetry.description}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}
