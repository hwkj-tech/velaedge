import { useEffect, useState } from 'react';
import { FileInput, Plus, ShieldCheck } from 'lucide-react';

import type { PointMappingResponse, SavePointMappingRequest } from '../api/types';
import { DataTable, type DataTableColumn } from '../components/DataTable';
import { Drawer } from '../components/Drawer';
import './PointMappingsPage.css';

const fallbackPoints: PointMappingResponse[] = [
  {
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

export function PointMappingsPage({
  onSavePoint,
  points = fallbackPoints,
}: {
  onSavePoint?: (
    pointId: string,
    request: SavePointMappingRequest,
  ) => Promise<void> | void;
  points?: PointMappingResponse[];
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
  const columns = pointColumns(selectedPoint.pointId, setSelectedPointId);

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
      await onSavePoint?.(selectedPoint.pointId, request);
      setSaveState('saved');
    } catch {
      setSaveState('error');
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
          <button className="secondary-button" type="button">
            <FileInput size={15} aria-hidden="true" />
            批量导入
          </button>
          <button className="secondary-button" type="button">
            <ShieldCheck size={15} aria-hidden="true" />
            校验草稿
          </button>
          <button className="primary-button" type="button">
            <Plus size={15} aria-hidden="true" />
            新建点位
          </button>
        </div>
      </section>

      <div className="point-config-layout">
        <section className="panel point-table-panel">
          <div className="panel-header">
            <h3>点位配置表</h3>
            <span>{points.length} 个启用点位</span>
          </div>
          <DataTable columns={columns} getRowKey={(row) => row.pointId} rows={points} />
        </section>

        <Drawer
          subtitle="云端草稿，发布后边端 runtime 执行"
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
                {saveState === 'saving' ? '保存中' : '保存草稿'}
              </button>
            </>
          }
        >
          <DrawerSection
            fields={[
              ['Point ID', `${selectedPoint.pointId} / 草稿`],
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
      </div>
    </div>
  );
}

function pointColumns(
  selectedPointId: string,
  onSelectPoint: (pointId: string) => void,
): Array<DataTableColumn<PointMappingResponse>> {
  return [
    {
      key: 'pointId',
      header: 'Point ID',
      width: '110px',
      render: (row) => (
        <button
          aria-label={`选择点位 ${row.pointId}`}
          aria-pressed={row.pointId === selectedPointId}
          className="point-id-button"
          onClick={() => onSelectPoint(row.pointId)}
          type="button"
        >
          {row.pointId}
        </button>
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
      return '草稿已保存';
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
