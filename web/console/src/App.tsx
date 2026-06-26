import { useEffect, useState } from 'react';

import {
  fetchPointMappings,
  fetchReleaseList,
  fetchRuntimeStatus,
  fetchSummary,
  publishLatestRelease,
  savePointMapping,
} from './api/client';
import type {
  PointMappingResponse,
  ReleaseListResponse,
  RuntimeStatusResponse,
  SavePointMappingRequest,
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

interface ConsoleSnapshot {
  pointMappings: PointMappingResponse[];
  releaseList: ReleaseListResponse;
  runtimeStatus: RuntimeStatusResponse;
  summary: SummaryResponse;
}

export default function App() {
  const [activePage, setActivePage] = useState<PageKey>('dashboard');
  const [summary, setSummary] = useState(initialSummary);
  const [pointMappings, setPointMappings] = useState<PointMappingResponse[]>();
  const [releaseList, setReleaseList] = useState<ReleaseListResponse>();
  const [runtimeStatus, setRuntimeStatus] = useState<RuntimeStatusResponse>();
  const [loadState, setLoadState] = useState<'loading' | 'ready' | 'error'>(
    'loading',
  );

  const applySnapshot = (snapshot: ConsoleSnapshot) => {
    setSummary(snapshot.summary);
    setPointMappings(snapshot.pointMappings);
    setReleaseList(snapshot.releaseList);
    setRuntimeStatus(snapshot.runtimeStatus);
    setLoadState('ready');
  };

  const refreshConsoleData = async () => {
    applySnapshot(await loadConsoleSnapshot());
  };

  const handleSavePoint = async (
    pointId: string,
    request: SavePointMappingRequest,
  ) => {
    await savePointMapping(pointId, request);
    await refreshConsoleData();
  };

  const handlePublishLatestRelease = async () => {
    const nextReleaseList = await publishLatestRelease();
    const [nextSummary, nextPointMappings, nextRuntimeStatus] = await Promise.all([
      fetchSummary(),
      fetchPointMappings(),
      fetchRuntimeStatus(),
    ]);

    setSummary(nextSummary);
    setPointMappings(nextPointMappings);
    setReleaseList(nextReleaseList);
    setRuntimeStatus(nextRuntimeStatus);
    setLoadState('ready');
  };

  useEffect(() => {
    let mounted = true;

    loadConsoleSnapshot()
      .then((snapshot) => {
        if (mounted) {
          applySnapshot(snapshot);
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

  useEffect(() => {
    let mounted = true;
    const refreshRuntimeStatus = async () => {
      try {
        const nextRuntimeStatus = await fetchRuntimeStatus();
        if (mounted) {
          setRuntimeStatus(nextRuntimeStatus);
        }
      } catch {
        // Keep the last known runtime snapshot visible if polling misses once.
      }
    };
    const intervalId = window.setInterval(refreshRuntimeStatus, 5000);

    return () => {
      mounted = false;
      window.clearInterval(intervalId);
    };
  }, []);

  return (
    <AppShell activePage={activePage} onNavigate={setActivePage}>
      {renderPage(
        activePage,
        summary,
        loadState,
        handleSavePoint,
        handlePublishLatestRelease,
        pointMappings,
        releaseList,
        runtimeStatus,
      )}
    </AppShell>
  );
}

async function loadConsoleSnapshot(): Promise<ConsoleSnapshot> {
  const [summary, pointMappings, releaseList, runtimeStatus] = await Promise.all([
    fetchSummary(),
    fetchPointMappings(),
    fetchReleaseList(),
    fetchRuntimeStatus(),
  ]);

  return { pointMappings, releaseList, runtimeStatus, summary };
}

function renderPage(
  activePage: PageKey,
  summary: SummaryResponse,
  loadState: 'loading' | 'ready' | 'error',
  onSavePoint: (
    pointId: string,
    request: SavePointMappingRequest,
  ) => Promise<void>,
  onPublish: () => Promise<void>,
  pointMappings?: PointMappingResponse[],
  releaseList?: ReleaseListResponse,
  runtimeStatus?: RuntimeStatusResponse,
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
      return <PointMappingsPage onSavePoint={onSavePoint} points={pointMappings} />;
    case 'collectionTasks':
      return <CollectionTasksPage />;
    case 'algorithms':
      return <AlgorithmsPage />;
    case 'releases':
      return <ReleasesPage onPublish={onPublish} releaseList={releaseList} />;
    case 'runtimeStatus':
      return <RuntimeStatusPage runtimeStatus={runtimeStatus} />;
    case 'auditLog':
      return <AuditLogPage />;
    case 'agentAssistant':
      return <AgentAssistantPage />;
  }
}
