import { useState } from 'react';
import { GitCompare, Send, ShieldCheck } from 'lucide-react';

import type {
  EdgeNodeResponse,
  ManagementActionResponse,
  ReleaseListResponse,
} from '../api/types';
import { displayError } from '../utils/errors';
import { DataTable, type DataTableColumn } from '../components/DataTable';
import './PointMappingsPage.css';

const emptyReleaseList: ReleaseListResponse = {
  draftVersion: '-',
  validationStatus: '未校验',
  changeSummary: '暂无变更',
  rolloutPolicy: '未配置',
  applyResults: [],
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
  edges = [],
  onPublish,
  onShowDiff,
  onValidateRelease,
  releaseList = emptyReleaseList,
  selectedEdgeId: controlledEdgeId,
}: {
  edges?: EdgeNodeResponse[];
  onPublish?: (edgeId: string) => Promise<void> | void;
  onShowDiff?: (
    edgeId: string,
  ) => Promise<ManagementActionResponse> | ManagementActionResponse;
  onValidateRelease?: (
    edgeId: string,
  ) => Promise<ManagementActionResponse> | ManagementActionResponse;
  releaseList?: ReleaseListResponse;
  selectedEdgeId?: string;
}) {
  const selectedEdgeId = controlledEdgeId ?? edges[0]?.edgeId ?? '';
  const [publishState, setPublishState] = useState<
    'idle' | 'publishing' | 'published' | 'error'
  >('idle');
  const [toolbarMessage, setToolbarMessage] = useState('');
  const [actionState, setActionState] = useState<
    'idle' | 'diffing' | 'validating'
  >('idle');

  const handlePublish = async () => {
    setPublishState('publishing');
    setToolbarMessage('');

    try {
      await onPublish?.(selectedEdgeId);
      setPublishState('published');
    } catch (error) {
      setPublishState('error');
      setToolbarMessage(`发布失败：${displayError(error)}`);
    }
  };

  const handleShowDiff = async () => {
    setActionState('diffing');
    setToolbarMessage('');

    try {
      const result = await onShowDiff?.(selectedEdgeId);
      setToolbarMessage(result?.message ?? '配置差异摘要已生成');
    } catch (error) {
      setToolbarMessage(`配置差异摘要生成失败：${displayError(error)}`);
    } finally {
      setActionState('idle');
    }
  };

  const handleValidateRelease = async () => {
    setActionState('validating');
    setToolbarMessage('');

    try {
      const result = await onValidateRelease?.(selectedEdgeId);
      setToolbarMessage(
        result?.status ? `发布配置校验 ${result.status}` : '发布配置校验已通过',
      );
    } catch (error) {
      setToolbarMessage(`发布配置校验失败：${displayError(error)}`);
    } finally {
      setActionState('idle');
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>配置发布</h2>
          <p>校验、差异、发布与 runtime 回执。</p>
        </div>
        <div className="toolbar">
          {toolbarMessage ? (
            <span className="toolbar-status" role="status">
              {toolbarMessage}
            </span>
          ) : null}
          <button
            className="secondary-button"
            disabled={actionState === 'diffing' || !selectedEdgeId}
            onClick={() => {
              void handleShowDiff();
            }}
            type="button"
          >
            <GitCompare size={15} aria-hidden="true" />
            {actionState === 'diffing' ? '生成中' : '查看差异'}
          </button>
          <button
            className="secondary-button"
            disabled={actionState === 'validating' || !selectedEdgeId}
            onClick={() => {
              void handleValidateRelease();
            }}
            type="button"
          >
            <ShieldCheck size={15} aria-hidden="true" />
            {actionState === 'validating' ? '校验中' : '校验配置'}
          </button>
          <span className={`release-status ${publishState}`} role="status">
            {publishStatusText(publishState)}
          </span>
          <button
            className="primary-button"
            disabled={publishState === 'publishing' || !selectedEdgeId}
            onClick={handlePublish}
            type="button"
          >
            <Send size={15} aria-hidden="true" />
            {publishState === 'publishing' ? '发布中' : '创建发布'}
          </button>
        </div>
      </section>

      <section className="release-step-list" aria-label="发布流程">
        <article className="release-step">
          <span>待发布版本</span>
          <strong>{releaseList.draftVersion}</strong>
          <small>版本由当前配置草稿生成</small>
        </article>
        <article className="release-step">
          <span>校验状态</span>
          <strong>{releaseList.validationStatus}</strong>
          <small>发布前校验协议、点位和流水线</small>
        </article>
        <article className="release-step">
          <span>变更摘要</span>
          <strong>{releaseList.changeSummary}</strong>
          <small>影响范围以云端差异结果为准</small>
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
          emptyMessage="暂无边端应用回执"
          getRowKey={(row) => row.edgeId}
          rows={releaseList.applyResults}
        />
      </section>
    </div>
  );
}

function publishStatusText(
  publishState: 'idle' | 'publishing' | 'published' | 'error',
) {
  switch (publishState) {
    case 'publishing':
      return '发布中';
    case 'published':
      return '已创建发布，等待 runtime 回报';
    case 'error':
      return '发布失败';
    case 'idle':
      return '';
  }
}
