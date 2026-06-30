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

export function AppShell({
  activePage,
  children,
  onNavigate,
}: {
  activePage: PageKey;
  children: ReactNode;
  onNavigate: (page: PageKey) => void;
}) {
  const activeTitle = pageTitleByKey.get(activePage) ?? 'Dashboard';

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
              3 个边端在线
            </span>
            <span className="status-pill">项目: demo-plant</span>
            <span className="status-pill">环境: staging</span>
            <span className="status-pill warning">待发布 v2026.06.26-002</span>
          </div>
        </header>

        <main className="content-shell">{children}</main>
      </div>
    </div>
  );
}
