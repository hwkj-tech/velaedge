import { useEffect, useMemo, useState } from 'react';
import {
  ChevronLeft,
  ChevronRight,
  Eye,
  RefreshCw,
  Search,
  X,
} from 'lucide-react';

import type { AuditRecordResponse } from '../api/types';
import { Modal } from '../components/Modal';
import { displayError } from '../utils/errors';
import './AuditLogPage.css';

const defaultPageSize = 12;

export function AuditLogPage({
  auditRecords = [],
  onRefresh,
  pageSize = defaultPageSize,
}: {
  auditRecords?: AuditRecordResponse[];
  onRefresh?: () => Promise<AuditRecordResponse[]>;
  pageSize?: number;
}) {
  const [records, setRecords] = useState(auditRecords);
  const [query, setQuery] = useState('');
  const [resultFilter, setResultFilter] = useState('all');
  const [actionFilter, setActionFilter] = useState('all');
  const [page, setPage] = useState(1);
  const [selectedRecord, setSelectedRecord] = useState<AuditRecordResponse>();
  const [refreshState, setRefreshState] = useState<
    'idle' | 'refreshing' | 'success' | 'error'
  >('idle');
  const [refreshMessage, setRefreshMessage] = useState('');

  useEffect(() => {
    setRecords(auditRecords);
  }, [auditRecords]);

  const actions = useMemo(
    () => Array.from(new Set(records.map((record) => record.action))).sort(),
    [records],
  );
  const results = useMemo(
    () => Array.from(new Set(records.map((record) => record.result))).sort(),
    [records],
  );
  const filteredRecords = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    return records.filter((record) => {
      if (actionFilter !== 'all' && record.action !== actionFilter) return false;
      if (resultFilter !== 'all' && record.result !== resultFilter) return false;
      if (!normalizedQuery) return true;
      return [record.actor, record.action, record.target, record.result]
        .join(' ')
        .toLocaleLowerCase()
        .includes(normalizedQuery);
    });
  }, [actionFilter, query, records, resultFilter]);
  const totalPages = Math.max(1, Math.ceil(filteredRecords.length / pageSize));
  const currentPage = Math.min(page, totalPages);
  const visibleRecords = filteredRecords.slice(
    (currentPage - 1) * pageSize,
    currentPage * pageSize,
  );

  useEffect(() => {
    setPage(1);
  }, [actionFilter, query, resultFilter]);

  const refresh = async () => {
    if (!onRefresh) return;
    setRefreshState('refreshing');
    setRefreshMessage('');
    try {
      const nextRecords = await onRefresh();
      setRecords(nextRecords);
      setRefreshState('success');
      setRefreshMessage(`已同步 ${nextRecords.length} 条审计记录`);
    } catch (error) {
      setRefreshState('error');
      setRefreshMessage(`刷新失败：${displayError(error)}`);
    }
  };

  return (
    <div className="page-stack audit-page">
      <section className="page-intro">
        <div>
          <h2>审计日志</h2>
          <p>追踪配置、审批、发布和 Runtime 回执。</p>
        </div>
        <div className="toolbar">
          {refreshMessage ? (
            <span
              className={refreshState === 'error' ? 'editor-status error' : 'editor-status'}
              role="status"
            >
              {refreshMessage}
            </span>
          ) : null}
          <button
            className="secondary-button"
            disabled={!onRefresh || refreshState === 'refreshing'}
            onClick={() => void refresh()}
            type="button"
          >
            <RefreshCw
              aria-hidden="true"
              className={refreshState === 'refreshing' ? 'audit-spin' : undefined}
              size={15}
            />
            {refreshState === 'refreshing' ? '同步中' : '刷新'}
          </button>
        </div>
      </section>

      <section className="panel audit-panel">
        <div className="panel-header audit-panel-header">
          <div>
            <h3>审计事件</h3>
            <span>{filteredRecords.length} / {records.length} 条记录</span>
          </div>
          <div className="audit-filters" aria-label="审计日志筛选">
            <label className="audit-search">
              <Search aria-hidden="true" size={16} />
              <input
                aria-label="搜索审计日志"
                onChange={(event) => setQuery(event.target.value)}
                placeholder="主体、动作或对象"
                value={query}
              />
            </label>
            <label>
              <span className="sr-only">动作筛选</span>
              <select
                aria-label="动作筛选"
                onChange={(event) => setActionFilter(event.target.value)}
                value={actionFilter}
              >
                <option value="all">全部动作</option>
                {actions.map((action) => (
                  <option key={action} value={action}>{action}</option>
                ))}
              </select>
            </label>
            <label>
              <span className="sr-only">结果筛选</span>
              <select
                aria-label="结果筛选"
                onChange={(event) => setResultFilter(event.target.value)}
                value={resultFilter}
              >
                <option value="all">全部结果</option>
                {results.map((result) => (
                  <option key={result} value={result}>{result}</option>
                ))}
              </select>
            </label>
          </div>
        </div>
        <div className="table-wrap">
          <table className="ops-table">
            <thead>
              <tr>
                <th>时间</th>
                <th>主体</th>
                <th>动作</th>
                <th>对象</th>
                <th>结果</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {visibleRecords.length === 0 ? (
                <tr>
                  <td className="table-empty-cell" colSpan={6}>
                    {records.length === 0 ? '暂无审计记录' : '没有符合筛选条件的审计记录'}
                  </td>
                </tr>
              ) : null}
              {visibleRecords.map((record) => (
                <tr key={`${record.createdAt}-${record.action}-${record.target}`}>
                  <td>
                    <time dateTime={record.createdAt}>{formatAuditTime(record)}</time>
                  </td>
                  <td>{record.actor}</td>
                  <td><code className="audit-action-code">{record.action}</code></td>
                  <td>{record.target}</td>
                  <td>
                    <span className={auditResultClass(record.result)}>
                      {record.result}
                    </span>
                  </td>
                  <td>
                    <button
                      aria-label={`查看审计事件 ${record.action} ${record.target}`}
                      className="icon-button audit-view-button"
                      onClick={() => setSelectedRecord(record)}
                      title="查看详情"
                      type="button"
                    >
                      <Eye aria-hidden="true" size={15} />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="pagination-bar" aria-label="审计日志分页">
          <span>第 {currentPage} / {totalPages} 页</span>
          <div className="row-actions">
            <button
              className="secondary-button compact"
              disabled={currentPage === 1}
              onClick={() => setPage((value) => Math.max(1, value - 1))}
              type="button"
            >
              <ChevronLeft aria-hidden="true" size={14} />
              上一页
            </button>
            <button
              className="secondary-button compact"
              disabled={currentPage === totalPages}
              onClick={() => setPage((value) => Math.min(totalPages, value + 1))}
              type="button"
            >
              下一页
              <ChevronRight aria-hidden="true" size={14} />
            </button>
          </div>
        </div>
      </section>

      {selectedRecord ? (
        <Modal onClose={() => setSelectedRecord(undefined)}>
          <section
            aria-label="审计事件详情"
            className="modal-panel audit-detail-modal"
            role="dialog"
          >
            <div className="modal-header">
              <div>
                <span className="modal-eyebrow">AUDIT EVENT</span>
                <h3>{selectedRecord.action}</h3>
                <p>{selectedRecord.target}</p>
              </div>
              <button
                aria-label="关闭审计详情"
                className="icon-button"
                onClick={() => setSelectedRecord(undefined)}
                type="button"
              >
                <X aria-hidden="true" size={16} />
              </button>
            </div>
            <dl className="audit-detail-list">
              <AuditDetail label="记录时间" value={formatAuditTimestamp(selectedRecord.createdAt)} />
              <AuditDetail label="操作主体" value={selectedRecord.actor} />
              <AuditDetail label="动作" value={selectedRecord.action} code />
              <AuditDetail label="目标对象" value={selectedRecord.target} code />
              <AuditDetail label="执行结果" value={selectedRecord.result} />
            </dl>
            <div className="drawer-footer">
              <span className="editor-status">只读审计记录</span>
              <button
                className="secondary-button"
                onClick={() => setSelectedRecord(undefined)}
                type="button"
              >
                关闭
              </button>
            </div>
          </section>
        </Modal>
      ) : null}
    </div>
  );
}

function AuditDetail({
  code = false,
  label,
  value,
}: {
  code?: boolean;
  label: string;
  value: string;
}) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{code ? <code>{value}</code> : value}</dd>
    </div>
  );
}

function auditResultClass(result: string) {
  if (result === '成功' || result === '已完成' || result === '通过') return 'tag ok';
  if (result === '失败' || result === '拒绝') return 'tag danger';
  return 'tag warn';
}

function formatAuditTime(record: AuditRecordResponse) {
  const timestamp = new Date(record.createdAt);
  if (Number.isNaN(timestamp.getTime())) return record.time;
  return new Intl.DateTimeFormat('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
    month: '2-digit',
    day: '2-digit',
    second: '2-digit',
  }).format(timestamp);
}

function formatAuditTimestamp(value: string) {
  const timestamp = new Date(value);
  if (Number.isNaN(timestamp.getTime())) return value;
  return new Intl.DateTimeFormat('zh-CN', {
    dateStyle: 'medium',
    timeStyle: 'medium',
  }).format(timestamp);
}
