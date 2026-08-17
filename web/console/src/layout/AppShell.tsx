import {
  Activity,
  Boxes,
  Database,
  FolderKanban,
  LayoutDashboard,
  LogOut,
  RadioTower,
  ScrollText,
  Workflow,
  UserRound,
  type LucideIcon,
} from 'lucide-react';
import type { ReactNode } from 'react';

import type { AuthStatusResponse } from '../api/types';
import './AppShell.css';

export type PageKey =
  | 'dashboard'
  | 'edges'
  | 'edgeConfig'
  | 'projects'
  | 'products'
  | 'points'
  | 'deviceModels'
  | 'protocolConnections'
  | 'dataConfigs'
  | 'discovery'
  | 'runtimeStatus'
  | 'auditLog'
  | 'aiIntegration';

interface NavItem {
  key: PageKey;
  label: string;
  icon: LucideIcon;
}

export const navItems: NavItem[] = [
  { key: 'dashboard', label: 'Dashboard', icon: LayoutDashboard },
  { key: 'projects', label: '项目管理', icon: FolderKanban },
  { key: 'products', label: '产品管理', icon: Boxes },
  { key: 'points', label: '点位管理', icon: Database },
  { key: 'edges', label: '边端管理', icon: RadioTower },
  { key: 'runtimeStatus', label: '运行状态', icon: Activity },
  { key: 'auditLog', label: '审计日志', icon: ScrollText },
  { key: 'aiIntegration', label: 'AI 集成', icon: Workflow },
];

const pageTitleByKey = new Map(navItems.map((item) => [item.key, item.label]));
pageTitleByKey.set('edgeConfig', '边端配置');
pageTitleByKey.set('protocolConnections', '协议连接');
pageTitleByKey.set('dataConfigs', '数据上报');

export interface PlatformStatus {
  environment: string;
  onlineEdgeCount: number;
  pendingReleaseCount: number;
  project: string;
}

export function AppShell({
  activePage,
  children,
  onNavigate,
  onLogout,
  principal,
}: {
  activePage: PageKey;
  children: ReactNode;
  onNavigate: (page: PageKey) => void;
  onLogout?: () => void;
  platformStatus?: PlatformStatus;
  principal?: AuthStatusResponse;
}) {
  const activeTitle = pageTitleByKey.get(activePage) ?? 'Dashboard';

  return (
    <div className="app-shell">
      <aside className="sidebar" aria-label="主导航">
        <div className="brand">
          <div className="brand-mark" aria-hidden="true">
            <img alt="" src="/velaedge-mark.svg" />
          </div>
          <div><strong>VELAEDGE</strong><span>Edge Intelligence Fabric</span><span className="sr-only">VelaEdge</span></div>
        </div>

        <div className="nav-caption">CONTROL CENTER</div>

        <nav className="nav-list">
          {navItems.map(({ key, label, icon: Icon }) => (
            <button
              aria-current={activePage === key ? 'page' : undefined}
              className={activePage === key ? 'nav-item active' : 'nav-item'}
              key={key}
              onClick={() => onNavigate(key)}
              type="button"
            >
              <Icon size={16} aria-hidden="true" />
              <span>{label}</span>
            </button>
          ))}
        </nav>

        <div className="sidebar-footer">
          <div className="agent-pulse"><span /> Policy Engine</div>
          <strong>系统运行正常</strong>
          <small>v2.4.0 · 安全策略已启用</small>
        </div>
      </aside>

      <div className="main">
        <header className="topbar">
          <div>
            <span className="breadcrumb">VELAEDGE / {activeTitle}</span>
            <h1>{activeTitle}</h1>
          </div>

          <div className="status-strip" aria-label="账户操作">
            {principal ? (
              <span className="principal-status" title={principal.subject}>
                <UserRound size={14} aria-hidden="true" />
                <span>{principal.subject}</span>
                <small>{roleLabel(principal.role)}</small>
              </span>
            ) : null}
            {principal?.authenticationEnabled && onLogout ? (
              <button
                aria-label="退出控制台"
                className="icon-command"
                onClick={onLogout}
                title="退出控制台"
                type="button"
              >
                <LogOut size={15} aria-hidden="true" />
              </button>
            ) : null}
          </div>
        </header>

        <main className="content-shell">{children}</main>
      </div>
    </div>
  );
}

function roleLabel(role: AuthStatusResponse['role']): string {
  return { admin: '管理员', operator: '操作员', viewer: '只读' }[role];
}
