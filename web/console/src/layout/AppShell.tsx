import {
  Activity,
  Bot,
  Cpu,
  LayoutDashboard,
  RadioTower,
  Search,
  ScrollText,
  ShieldCheck,
  type LucideIcon,
} from 'lucide-react';
import type { ReactNode } from 'react';

import './AppShell.css';

export type PageKey =
  | 'dashboard'
  | 'edges'
  | 'edgeConfig'
  | 'deviceModels'
  | 'protocolConnections'
  | 'dataConfigs'
  | 'discovery'
  | 'releases'
  | 'runtimeStatus'
  | 'auditLog'
  | 'agentAssistant';

interface NavItem {
  key: PageKey;
  label: string;
  icon: LucideIcon;
}

export const navItems: NavItem[] = [
  { key: 'dashboard', label: 'Dashboard', icon: LayoutDashboard },
  { key: 'edges', label: '边端管理', icon: RadioTower },
  { key: 'deviceModels', label: '设备模型', icon: Cpu },
  { key: 'discovery', label: '点位探测', icon: Search },
  { key: 'runtimeStatus', label: '运行状态', icon: Activity },
  { key: 'auditLog', label: '审计日志', icon: ScrollText },
  { key: 'agentAssistant', label: 'Agent 助手', icon: Bot },
];

const pageTitleByKey = new Map(navItems.map((item) => [item.key, item.label]));
pageTitleByKey.set('edgeConfig', '边端配置');
pageTitleByKey.set('protocolConnections', '协议连接');
pageTitleByKey.set('dataConfigs', '数据上报');
pageTitleByKey.set('releases', '配置发布');

export interface PlatformStatus {
  environment: string;
  onlineEdgeCount: number;
  pendingReleaseCount: number;
  project: string;
}

const defaultPlatformStatus: PlatformStatus = {
  environment: 'staging',
  onlineEdgeCount: 0,
  pendingReleaseCount: 0,
  project: 'demo-plant',
};

export function AppShell({
  activePage,
  children,
  onNavigate,
  platformStatus = defaultPlatformStatus,
}: {
  activePage: PageKey;
  children: ReactNode;
  onNavigate: (page: PageKey) => void;
  platformStatus?: PlatformStatus;
}) {
  const activeTitle = pageTitleByKey.get(activePage) ?? 'Dashboard';
  const releaseStatus =
    platformStatus.pendingReleaseCount > 0
      ? `${platformStatus.pendingReleaseCount} 个配置待发布`
      : '配置已同步';

  return (
    <div className="app-shell">
      <aside className="sidebar" aria-label="主导航">
        <div className="brand">
          <strong>EdgeOps Cloud</strong>
          <span>边云一体化管理台</span>
        </div>

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
      </aside>

      <div className="main">
        <header className="topbar">
          <div>
            <span className="breadcrumb">云端配置 / {activeTitle}</span>
            <h1>{activeTitle}</h1>
          </div>

          <div className="status-strip" aria-label="平台状态">
            <span className="status-pill online">
              <ShieldCheck size={14} aria-hidden="true" />
              {platformStatus.onlineEdgeCount} 个边端在线
            </span>
            <span className="status-pill">项目: {platformStatus.project}</span>
            <span className="status-pill">环境: {platformStatus.environment}</span>
            <span
              className={
                platformStatus.pendingReleaseCount > 0
                  ? 'status-pill warning'
                  : 'status-pill online'
              }
            >
              {releaseStatus}
            </span>
          </div>
        </header>

        <main className="content-shell">{children}</main>
      </div>
    </div>
  );
}
