import { useState } from 'react';
import { Plus } from 'lucide-react';

import type { DeviceModelResponse } from '../api/types';

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
  onCreateDeviceModel?: () => Promise<DeviceModelResponse> | DeviceModelResponse;
}) {
  const activeModel = deviceModels[0] ?? fallbackDeviceModels[0];
  const [toolbarMessage, setToolbarMessage] = useState('');
  const [actionState, setActionState] = useState<'idle' | 'creating'>('idle');

  const handleCreateDeviceModel = async () => {
    setActionState('creating');
    setToolbarMessage('');

    try {
      const created = await onCreateDeviceModel?.();
      setToolbarMessage(
        created
          ? `已创建设备模型草稿 ${created.deviceType}`
          : '已创建设备模型草稿',
      );
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
            先定义设备语义，再在点位配置中绑定具体 Modbus、OPC UA、MQTT 或模拟地址。
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
            disabled={actionState === 'creating'}
            onClick={() => {
              void handleCreateDeviceModel();
            }}
            type="button"
          >
            <Plus size={15} aria-hidden="true" />
            {actionState === 'creating' ? '创建中' : '新建设备模型'}
          </button>
        </div>
      </section>

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
