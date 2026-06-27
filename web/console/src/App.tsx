import { useEffect, useState } from 'react';

import {
  fetchAlgorithms,
  fetchAuditRecords,
  fetchCollectionTasks,
  fetchDeviceModels,
  fetchEdgeCollectionTasks,
  fetchEdgePointMappings,
  fetchEdgeNodes,
  fetchPointMappings,
  fetchProtocolConnections,
  fetchReleaseList,
  fetchRuntimeStatus,
  fetchSummary,
  publishLatestRelease,
  saveEdgeCollectionTask,
  saveEdgePointMapping,
} from './api/client';
import type {
  AlgorithmResponse,
  AuditRecordResponse,
  CollectionTaskResponse,
  DeviceModelResponse,
  EdgeNodeResponse,
  PointMappingResponse,
  ProtocolConnectionResponse,
  ReleaseListResponse,
  RuntimeStatusResponse,
  SaveCollectionTaskRequest,
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
  algorithms: AlgorithmResponse[];
  auditRecords: AuditRecordResponse[];
  collectionTasks: CollectionTaskResponse[];
  deviceModels: DeviceModelResponse[];
  edgeNodes: EdgeNodeResponse[];
  pointMappings: PointMappingResponse[];
  protocolConnections: ProtocolConnectionResponse[];
  releaseList: ReleaseListResponse;
  runtimeStatus: RuntimeStatusResponse;
  summary: SummaryResponse;
}

export default function App() {
  const [activePage, setActivePage] = useState<PageKey>('dashboard');
  const [summary, setSummary] = useState(initialSummary);
  const [edgeNodes, setEdgeNodes] = useState<EdgeNodeResponse[]>();
  const [deviceModels, setDeviceModels] = useState<DeviceModelResponse[]>();
  const [protocolConnections, setProtocolConnections] =
    useState<ProtocolConnectionResponse[]>();
  const [pointMappings, setPointMappings] = useState<PointMappingResponse[]>();
  const [selectedPointEdgeId, setSelectedPointEdgeId] = useState('edge-dev');
  const [collectionTasks, setCollectionTasks] = useState<CollectionTaskResponse[]>();
  const [selectedCollectionEdgeId, setSelectedCollectionEdgeId] = useState('edge-dev');
  const [algorithms, setAlgorithms] = useState<AlgorithmResponse[]>();
  const [releaseList, setReleaseList] = useState<ReleaseListResponse>();
  const [runtimeStatus, setRuntimeStatus] = useState<RuntimeStatusResponse>();
  const [auditRecords, setAuditRecords] = useState<AuditRecordResponse[]>();
  const [loadState, setLoadState] = useState<'loading' | 'ready' | 'error'>(
    'loading',
  );

  const applySnapshot = (snapshot: ConsoleSnapshot) => {
    setSummary(snapshot.summary);
    setEdgeNodes(snapshot.edgeNodes);
    setDeviceModels(snapshot.deviceModels);
    setProtocolConnections(snapshot.protocolConnections);
    setPointMappings(snapshot.pointMappings);
    setCollectionTasks(snapshot.collectionTasks);
    setAlgorithms(snapshot.algorithms);
    setReleaseList(snapshot.releaseList);
    setRuntimeStatus(snapshot.runtimeStatus);
    setAuditRecords(snapshot.auditRecords);
    setLoadState('ready');
  };

  const refreshConsoleData = async () => {
    applySnapshot(await loadConsoleSnapshot());
  };

  const handleSavePoint = async (
    edgeId: string,
    pointId: string,
    request: SavePointMappingRequest,
  ) => {
    await saveEdgePointMapping(edgeId, pointId, request);
    const [nextPointMappings, nextReleaseList] = await Promise.all([
      fetchEdgePointMappings(edgeId),
      fetchReleaseList(),
    ]);
    setPointMappings(nextPointMappings);
    setReleaseList(nextReleaseList);
    setSelectedPointEdgeId(edgeId);
  };

  const handleSelectPointEdge = async (edgeId: string) => {
    setSelectedPointEdgeId(edgeId);
    setPointMappings(await fetchEdgePointMappings(edgeId));
  };

  const handleSaveCollectionTask = async (
    edgeId: string,
    taskId: string,
    request: SaveCollectionTaskRequest,
  ) => {
    await saveEdgeCollectionTask(edgeId, taskId, request);
    const [nextCollectionTasks, nextReleaseList] = await Promise.all([
      fetchEdgeCollectionTasks(edgeId),
      fetchReleaseList(),
    ]);
    setCollectionTasks(nextCollectionTasks);
    setReleaseList(nextReleaseList);
    setSelectedCollectionEdgeId(edgeId);
  };

  const handleSelectCollectionEdge = async (edgeId: string) => {
    setSelectedCollectionEdgeId(edgeId);
    setCollectionTasks(await fetchEdgeCollectionTasks(edgeId));
  };

  const handlePublishLatestRelease = async (edgeId: string) => {
    await publishLatestRelease(edgeId);
    await refreshConsoleData();
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
        handleSelectPointEdge,
        selectedPointEdgeId,
        handleSaveCollectionTask,
        handleSelectCollectionEdge,
        selectedCollectionEdgeId,
        handlePublishLatestRelease,
        edgeNodes,
        deviceModels,
        protocolConnections,
        pointMappings,
        collectionTasks,
        algorithms,
        releaseList,
        runtimeStatus,
        auditRecords,
      )}
    </AppShell>
  );
}

async function loadConsoleSnapshot(): Promise<ConsoleSnapshot> {
  const [
    summary,
    edgeNodes,
    deviceModels,
    protocolConnections,
    pointMappings,
    collectionTasks,
    algorithms,
    releaseList,
    runtimeStatus,
    auditRecords,
  ] = await Promise.all([
    fetchSummary(),
    fetchEdgeNodes(),
    fetchDeviceModels(),
    fetchProtocolConnections(),
    fetchPointMappings(),
    fetchCollectionTasks(),
    fetchAlgorithms(),
    fetchReleaseList(),
    fetchRuntimeStatus(),
    fetchAuditRecords(),
  ]);

  return {
    algorithms,
    auditRecords,
    collectionTasks,
    deviceModels,
    edgeNodes,
    pointMappings,
    protocolConnections,
    releaseList,
    runtimeStatus,
    summary,
  };
}

function renderPage(
  activePage: PageKey,
  summary: SummaryResponse,
  loadState: 'loading' | 'ready' | 'error',
  onSavePoint: (
    edgeId: string,
    pointId: string,
    request: SavePointMappingRequest,
  ) => Promise<void>,
  onSelectPointEdge: (edgeId: string) => Promise<void>,
  selectedPointEdgeId: string,
  onSaveCollectionTask: (
    edgeId: string,
    taskId: string,
    request: SaveCollectionTaskRequest,
  ) => Promise<void>,
  onSelectCollectionEdge: (edgeId: string) => Promise<void>,
  selectedCollectionEdgeId: string,
  onPublish: (edgeId: string) => Promise<void>,
  edgeNodes?: EdgeNodeResponse[],
  deviceModels?: DeviceModelResponse[],
  protocolConnections?: ProtocolConnectionResponse[],
  pointMappings?: PointMappingResponse[],
  collectionTasks?: CollectionTaskResponse[],
  algorithms?: AlgorithmResponse[],
  releaseList?: ReleaseListResponse,
  runtimeStatus?: RuntimeStatusResponse,
  auditRecords?: AuditRecordResponse[],
) {
  switch (activePage) {
    case 'dashboard':
      return <DashboardPage loadState={loadState} summary={summary} />;
    case 'edges':
      return <EdgeNodesPage edges={edgeNodes} />;
    case 'deviceModels':
      return <DeviceModelsPage deviceModels={deviceModels} />;
    case 'protocolConnections':
      return <ProtocolConnectionsPage connections={protocolConnections} />;
    case 'pointMappings':
      return (
        <PointMappingsPage
          edges={edgeNodes}
          onSavePoint={onSavePoint}
          onSelectEdge={onSelectPointEdge}
          points={pointMappings}
          selectedEdgeId={selectedPointEdgeId}
        />
      );
    case 'collectionTasks':
      return (
        <CollectionTasksPage
          edges={edgeNodes}
          onSaveTask={onSaveCollectionTask}
          onSelectEdge={onSelectCollectionEdge}
          selectedEdgeId={selectedCollectionEdgeId}
          tasks={collectionTasks}
        />
      );
    case 'algorithms':
      return <AlgorithmsPage algorithms={algorithms} />;
    case 'releases':
      return (
        <ReleasesPage
          edges={edgeNodes}
          onPublish={onPublish}
          releaseList={releaseList}
        />
      );
    case 'runtimeStatus':
      return <RuntimeStatusPage runtimeStatus={runtimeStatus} />;
    case 'auditLog':
      return <AuditLogPage auditRecords={auditRecords} />;
    case 'agentAssistant':
      return <AgentAssistantPage />;
  }
}
