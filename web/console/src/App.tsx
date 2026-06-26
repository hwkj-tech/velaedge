import { useEffect, useState } from 'react';

import { fetchPointMappings, fetchReleaseList, fetchSummary } from './api/client';
import type {
  PointMappingResponse,
  ReleaseListResponse,
  SummaryResponse,
} from './api/types';
import { AppShell, type PageKey } from './layout/AppShell';
import { AgentAssistantPage } from './pages/AgentAssistantPage';
import { AlgorithmsPage } from './pages/AlgorithmsPage';
import { AuditLogPage } from './pages/AuditLogPage';
import { CollectionTasksPage } from './pages/CollectionTasksPage';
import { DashboardPage } from './pages/DashboardPage';
import { DeviceModelsPage } from './pages/DeviceModelsPage';
import { EdgeNodesPage } from './pages/EdgeNodesPage';
import { PointMappingsPage } from './pages/PointMappingsPage';
import { ProtocolConnectionsPage } from './pages/ProtocolConnectionsPage';
import { ReleasesPage } from './pages/ReleasesPage';
import { RuntimeStatusPage } from './pages/RuntimeStatusPage';

const initialSummary: SummaryResponse = {
  edge_count: 0,
  pending_release_count: 0,
};

export default function App() {
  const [activePage, setActivePage] = useState<PageKey>('dashboard');
  const [summary, setSummary] = useState(initialSummary);
  const [pointMappings, setPointMappings] = useState<PointMappingResponse[]>();
  const [releaseList, setReleaseList] = useState<ReleaseListResponse>();
  const [loadState, setLoadState] = useState<'loading' | 'ready' | 'error'>(
    'loading',
  );

  useEffect(() => {
    let mounted = true;

    Promise.all([fetchSummary(), fetchPointMappings(), fetchReleaseList()])
      .then(([nextSummary, nextPointMappings, nextReleaseList]) => {
        if (mounted) {
          setSummary(nextSummary);
          setPointMappings(nextPointMappings);
          setReleaseList(nextReleaseList);
          setLoadState('ready');
        }
      })
      .catch(() => {
        if (mounted) {
          setLoadState('error');
        }
      });

    return () => {
      mounted = false;
    };
  }, []);

  return (
    <AppShell activePage={activePage} onNavigate={setActivePage}>
      {renderPage(activePage, summary, loadState, pointMappings, releaseList)}
    </AppShell>
  );
}

function renderPage(
  activePage: PageKey,
  summary: SummaryResponse,
  loadState: 'loading' | 'ready' | 'error',
  pointMappings?: PointMappingResponse[],
  releaseList?: ReleaseListResponse,
) {
  switch (activePage) {
    case 'dashboard':
      return <DashboardPage loadState={loadState} summary={summary} />;
    case 'edges':
      return <EdgeNodesPage />;
    case 'deviceModels':
      return <DeviceModelsPage />;
    case 'protocolConnections':
      return <ProtocolConnectionsPage />;
    case 'pointMappings':
      return <PointMappingsPage points={pointMappings} />;
    case 'collectionTasks':
      return <CollectionTasksPage />;
    case 'algorithms':
      return <AlgorithmsPage />;
    case 'releases':
      return <ReleasesPage releaseList={releaseList} />;
    case 'runtimeStatus':
      return <RuntimeStatusPage />;
    case 'auditLog':
      return <AuditLogPage />;
    case 'agentAssistant':
      return <AgentAssistantPage />;
  }
}
