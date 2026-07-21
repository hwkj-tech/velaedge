import type { ReactNode } from 'react';
import { X } from 'lucide-react';
import { Modal } from './Modal';

export function Drawer({
  children,
  footer,
  onClose,
  subtitle,
  title,
}: {
  children: ReactNode;
  footer?: ReactNode;
  onClose?: () => void;
  subtitle?: string;
  title: string;
}) {
  return (
    <Modal onClose={onClose}>
      <aside aria-label={title} className="drawer" role="dialog">
        <header className="drawer-header">
          <div>
            <h3>{title}</h3>
            {subtitle && <p>{subtitle}</p>}
          </div>
          {onClose ? (
            <button
              aria-label="关闭"
              className="icon-button"
              onClick={onClose}
              type="button"
            >
              <X size={16} aria-hidden="true" />
            </button>
          ) : null}
        </header>
        <div className="drawer-body">{children}</div>
        {footer && <footer className="drawer-footer">{footer}</footer>}
      </aside>
    </Modal>
  );
}
