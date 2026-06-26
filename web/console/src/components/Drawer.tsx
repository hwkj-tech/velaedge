import type { ReactNode } from 'react';

export function Drawer({
  children,
  footer,
  subtitle,
  title,
}: {
  children: ReactNode;
  footer?: ReactNode;
  subtitle?: string;
  title: string;
}) {
  return (
    <aside className="drawer" aria-label={title}>
      <header className="drawer-header">
        <div>
          <h3>{title}</h3>
          {subtitle && <p>{subtitle}</p>}
        </div>
      </header>
      <div className="drawer-body">{children}</div>
      {footer && <footer className="drawer-footer">{footer}</footer>}
    </aside>
  );
}
