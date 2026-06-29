import { ChevronLeft, ChevronRight } from 'lucide-react';

export function PaginationBar({
  ariaLabel,
  currentPage,
  onNext,
  onPrevious,
  totalPages,
}: {
  ariaLabel: string;
  currentPage: number;
  onNext: () => void;
  onPrevious: () => void;
  totalPages: number;
}) {
  return (
    <div className="pagination-bar" aria-label={ariaLabel}>
      <span>
        第 {currentPage} / {totalPages} 页
      </span>
      <div className="row-actions">
        <button
          className="secondary-button compact"
          disabled={currentPage === 1}
          onClick={onPrevious}
          type="button"
        >
          <ChevronLeft size={14} aria-hidden="true" />
          上一页
        </button>
        <button
          className="secondary-button compact"
          disabled={currentPage === totalPages}
          onClick={onNext}
          type="button"
        >
          <ChevronRight size={14} aria-hidden="true" />
          下一页
        </button>
      </div>
    </div>
  );
}
