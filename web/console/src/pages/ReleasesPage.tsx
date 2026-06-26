import { GitCompare, Send, ShieldCheck } from 'lucide-react';

import type { ReleaseListResponse } from '../api/types';
import { DataTable, type DataTableColumn } from '../components/DataTable';
import './PointMappingsPage.css';

const fallbackReleaseList: ReleaseListResponse = {
  draftVersion: '2026.06.26-001',
  validationStatus: '已通过',
  changeSummary: '新增 2 个 Modbus 点位',
  rolloutPolicy: '先灰度 edge-lab-03',
  applyResults: [
    {
      edgeId: 'edge-shanghai-01',
      desiredVersion: '2026.06.26-001',
      reportedVersion: '2026.06.26-001',
      result: '已应用',
      heartbeat: '18 秒前',
    },
    {
      edgeId: 'edge-suzhou-02',
      desiredVersion: '2026.06.26-001',
      reportedVersion: '2026.06.26-001',
      result: '已应用',
      heartbeat: '24 秒前',
    },
    {
      edgeId: 'edge-lab-03',
      desiredVersion: '2026.06.26-001',
      reportedVersion: '2026.06.25-004',
      result: '等待下发',
      heartbeat: '11 分钟前',
    },
  ],
};

const applyColumns: Array<DataTableColumn<ReleaseListResponse['applyResults'][number]>> = [
  { key: 'edgeId', header: 'Edge ID', width: '180px', render: (row) => row.edgeId },
  {
    key: 'desiredVersion',
    header: '期望版本',
    width: '160px',
    render: (row) => row.desiredVersion,
  },
  {
    key: 'reportedVersion',
    header: '上报版本',
    width: '160px',
    render: (row) => row.reportedVersion,
  },
  {
    key: 'result',
    header: '应用结果',
    width: '120px',
    render: (row) => (
      <span className={row.result === '已应用' ? 'tag ok' : 'tag warn'}>
        {row.result}
      </span>
    ),
  },
  { key: 'heartbeat', header: '心跳', width: '120px', render: (row) => row.heartbeat },
];

export function ReleasesPage({
  releaseList = fallbackReleaseList,
}: {
  releaseList?: ReleaseListResponse;
}) {
  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>配置发布</h2>
          <p>
            将云端草稿打包成边端配置版本，经过校验、审批、灰度和回执确认后再扩大发布范围。
          </p>
        </div>
        <div className="toolbar">
          <button className="secondary-button" type="button">
            <GitCompare size={15} aria-hidden="true" />
            查看差异
          </button>
          <button className="secondary-button" type="button">
            <ShieldCheck size={15} aria-hidden="true" />
            校验草稿
          </button>
          <button className="primary-button" type="button">
            <Send size={15} aria-hidden="true" />
            创建发布
          </button>
        </div>
      </section>

      <section className="release-step-list" aria-label="发布流程">
        <article className="release-step">
          <span>草稿版本</span>
          <strong>{releaseList.draftVersion}</strong>
          <small>点位与采集任务已生成</small>
        </article>
        <article className="release-step">
          <span>校验状态</span>
          <strong>{releaseList.validationStatus}</strong>
          <small>协议连接和点位地址有效</small>
        </article>
        <article className="release-step">
          <span>变更摘要</span>
          <strong>{releaseList.changeSummary}</strong>
          <small>影响 pump-1 采集任务</small>
        </article>
        <article className="release-step">
          <span>发布策略</span>
          <strong>{releaseList.rolloutPolicy}</strong>
          <small>确认回执后全量发布</small>
        </article>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>边端应用回执</h3>
          <span>desired / reported 对齐检查</span>
        </div>
        <DataTable
          columns={applyColumns}
          getRowKey={(row) => row.edgeId}
          rows={releaseList.applyResults}
        />
      </section>
    </div>
  );
}
