import { FileInput, Plus, ShieldCheck } from 'lucide-react';

import type { PointMappingResponse } from '../api/types';
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

const columns: Array<DataTableColumn<PointMappingResponse>> = [
  { key: 'pointId', header: 'Point ID', width: '110px', render: (row) => row.pointId },
  { key: 'address', header: '地址 / NodeId / Topic', width: '180px', render: (row) => row.address },
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

export function PointMappingsPage({
  points = fallbackPoints,
}: {
  points?: PointMappingResponse[];
}) {
  const selectedPoint = points[0];

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
              <button className="secondary-button" type="button">
                取消
              </button>
              <button className="primary-button" type="button">
                保存草稿
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
              ['地址类型', 'holding_register'],
              ['地址值', '40001'],
              ['数据类型', selectedPoint.valueType],
              ['读写类型', selectedPoint.readWrite],
              ['缩放系数', selectedPoint.scale],
              ['偏移量', '0'],
            ]}
            title="协议映射"
          />
          <DrawerSection
            fields={[
              ['采集周期', selectedPoint.interval],
              ['超时', '800ms'],
              ['重试次数', '2'],
              ['死区', '0.02'],
              ['缓存策略', 'local-first'],
            ]}
            title="采集策略"
          />
          <DrawerSection
            fields={[
              ['单位', selectedPoint.unit],
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
