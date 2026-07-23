import { useEffect, useState, type ReactNode } from 'react';

import { PaginationBar } from './PaginationBar';

export interface DataTableColumn<T> {
  key: string;
  header: string;
  width?: string;
  render: (row: T) => ReactNode;
}

export function DataTable<T>({
  ariaLabel = '列表分页',
  columns,
  emptyMessage = '暂无数据',
  getRowKey,
  pageSize,
  rows,
}: {
  ariaLabel?: string;
  columns: Array<DataTableColumn<T>>;
  emptyMessage?: string;
  getRowKey: (row: T) => string;
  pageSize?: number;
  rows: T[];
}) {
  const [page, setPage] = useState(1);
  const totalPages = pageSize ? Math.max(1, Math.ceil(rows.length / pageSize)) : 1;
  const currentPage = Math.min(page, totalPages);
  const visibleRows = pageSize
    ? rows.slice((currentPage - 1) * pageSize, currentPage * pageSize)
    : rows;

  useEffect(() => {
    setPage(1);
  }, [rows]);

  return (
    <>
      <div className="table-wrap">
        <table className="ops-table data-table">
          <colgroup>
            {columns.map((column) => (
              <col key={column.key} style={{ width: column.width }} />
            ))}
          </colgroup>
          <thead>
            <tr>
              {columns.map((column) => (
                <th key={column.key}>{column.header}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {visibleRows.length === 0 ? (
              <tr>
                <td className="table-empty-cell" colSpan={columns.length}>
                  {emptyMessage}
                </td>
              </tr>
            ) : null}
            {visibleRows.map((row) => (
              <tr key={getRowKey(row)}>
                {columns.map((column) => (
                  <td key={column.key}>{column.render(row)}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {pageSize && rows.length > pageSize ? (
        <PaginationBar
          ariaLabel={ariaLabel}
          currentPage={currentPage}
          onNext={() => setPage((value) => Math.min(totalPages, value + 1))}
          onPrevious={() => setPage((value) => Math.max(1, value - 1))}
          totalPages={totalPages}
        />
      ) : null}
    </>
  );
}
