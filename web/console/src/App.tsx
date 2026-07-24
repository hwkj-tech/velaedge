import { useEffect, useRef, useState, type DragEvent, type MouseEvent, type PointerEvent } from 'react';
import { Check, Trash2, X } from 'lucide-react';

import {
  bindEdgeProduct,
  createAgentProposal,
  createAgentKnowledgeDocument,
  createAlgorithmDraft,
  createCollectionTaskDraft,
  createEdgeDataConfig,
  createEdgeNode,
  createProduct as createProductApi,
  createProductVersion,
  createProject as createProjectApi,
  createPointSet as createPointSetApi,
  createDeviceModelDraft,
  createPointMappingDraft,
  fetchAuditRecords,
  fetchAuthStatus,
  fetchAgentProviderStatus,
  fetchAgentProposals,
  fetchAgentKnowledgeDocuments,
  fetchAgentConversations,
  fetchDeviceModels,
  fetchEdgeAlgorithms,
  fetchEdgeCollectionTasks,
  fetchEdgeDataConfigs,
  fetchEdgePointMappings,
  fetchEdgeProtocolConnections,
  fetchEdgeNodes,
  fetchPointSets,
  fetchProducts,
  fetchProductVersions,
  fetchProjects,
  fetchReleaseList,
  fetchRuntimeStatus,
  fetchSummary,
  fetchMqttUplink,
  fetchDiscoverySuggestions,
  generateEdgeAccessToken as generateEdgeAccessTokenApi,
  generateAgentSuggestions,
  publishLatestRelease,
  publishProductVersion as publishProductVersionApi,
  rollbackProductVersion as rollbackProductVersionApi,
  runAgentSafetyCheck,
  runConfigValidation,
  runReleaseDiff,
  reviewAgentProposal,
  saveAgentKnowledgeDocument,
  runDiscovery,
  saveMqttUplink,
  saveProduct as saveProductApi,
  saveProductVersion,
  saveProject as saveProjectApi,
  savePointSet as savePointSetApi,
  deleteEdgeAlgorithm,
  deleteAgentKnowledgeDocument,
  deleteAgentConversation,
  deleteEdgeCollectionTask,
  deleteEdgeDataConfig,
  deleteDeviceModel,
  deleteEdgeNode,
  deleteProduct as deleteProductApi,
  deleteProject as deleteProjectApi,
  deletePointSet as deletePointSetApi,
  deleteEdgePointMapping,
  deleteEdgeProtocolConnection,
  createEdgeProtocolConnection,
  saveDeviceModel,
  saveEdgeAlgorithm,
  saveEdgeCollectionTask,
  saveEdgeDataConfig,
  saveEdgePointMapping,
  saveEdgeProtocolConnection,
  sendAgentChat,
  setApiToken,
} from './api/client';
import type {
  AlgorithmResponse,
  AgentActionResponse,
  AgentChatRequest,
  AgentChatResponse,
  AgentConversationResponse,
  AgentKnowledgeDocumentResponse,
  AgentProposalResponse,
  AgentProviderStatusResponse,
  AuditRecordResponse,
  AuthStatusResponse,
  CollectionTaskResponse,
  CreateAlgorithmRequest,
  CreateAgentProposalRequest,
  CreateCollectionTaskRequest,
  CreateDeviceModelRequest,
  CreatePointMappingRequest,
  DiscoveryReportResponse,
  DeviceModelResponse,
  DataConfigPoint,
  DataConfigResponse,
  DataConfigVisualGraph,
  EdgeNodeResponse,
  ManagementActionResponse,
  MqttUplinkResponse,
  PointMappingResponse,
  PointMappingSuggestionResponse,
  PointSetResponse,
  ProductResponse,
  ProductVersionResponse,
  ProjectResponse,
  ProtocolConnectionResponse,
  ReleaseListResponse,
  RunDiscoveryRequest,
  RuntimeStatusResponse,
  ReviewAgentProposalRequest,
  SaveAlgorithmRequest,
  SaveAgentKnowledgeDocumentRequest,
  SaveCollectionTaskRequest,
  SaveDataConfigRequest,
  SaveDeviceModelRequest,
  CreateProtocolConnectionRequest,
  SavePointMappingRequest,
  SavePointSetRequest,
  SaveProductRequest,
  SaveProductVersionRequest,
  SaveProjectRequest,
  SaveProtocolConnectionRequest,
  SummaryResponse,
} from './api/types';
import { AppShell, type PageKey } from './layout/AppShell';
import { AgentAssistantPage } from './pages/AgentAssistantPage';
import { AlgorithmsPage } from './pages/AlgorithmsPage';
import { AuditLogPage } from './pages/AuditLogPage';
import { DashboardPage } from './pages/DashboardPage';
import { DataConfigsPage } from './pages/DataConfigsPage';
import { DeviceModelsPage } from './pages/DeviceModelsPage';
import { DiscoveryPage } from './pages/DiscoveryPage';
import {
  type CreateManagedEdgeRequest,
  type EdgeProductOption,
  EdgeNodesPage,
  type EdgeConfigSummary,
  type EdgeConfigTabKey,
} from './pages/EdgeNodesPage';
import { CollectionTasksPage } from './pages/CollectionTasksPage';
import { MqttUplinkPage } from './pages/MqttUplinkPage';
import { PointMappingsPage } from './pages/PointMappingsPage';
import { PointSetsPage } from './pages/PointSetsPage';
import { ProtocolConnectionsPage } from './pages/ProtocolConnectionsPage';
import { ReleasesPage } from './pages/ReleasesPage';
import { RuntimeStatusPage } from './pages/RuntimeStatusPage';
import { Modal } from './components/Modal';
import { AuthGate } from './components/AuthGate';
import { displayError } from './utils/errors';

const initialSummary: SummaryResponse = {
  edge_count: 0,
  pending_release_count: 0,
};

const defaultConfigEdgeId = 'edge-dev';
type EdgeConfigurationMode = 'configure' | 'list';
type EdgeTemplateId = string;
type ProductConfigTab =
  | 'basic'
  | 'points'
  | 'collection'
  | 'release';

interface ProjectDefinition {
  description: string;
  environment: string;
  owner: string;
  projectId: string;
  projectName: string;
}

export interface EdgeTemplateDefinition {
  algorithm: CreateAlgorithmRequest & { algorithmId: string };
  connection: CreateProtocolConnectionRequest;
  dataConfig: Omit<SaveDataConfigRequest, 'protocolConnectionId'>;
  description: string;
  highlights: string[];
  mqtt: MqttUplinkResponse;
  name: string;
  pointSetIds?: string[];
  points: Array<CreatePointMappingRequest & { pointId: string }>;
  productType: string;
  projectId: string;
  recommendedFor: string;
  task: CreateCollectionTaskRequest & { taskId: string };
  templateId: EdgeTemplateId;
  version: string;
}

interface EdgeProductBinding {
  bindingStatus: string;
  desiredVersion: string;
  edgeId: string;
  loadedAt: string;
  productId: EdgeTemplateId;
  projectId: string;
  reportedVersion: string;
}

type EdgeAccessTokens = Record<string, string>;

const configurationPages = new Set<PageKey>(['edgeConfig']);
const deprecatedGlobalConfigurationPages = new Set<PageKey>([
  'protocolConnections',
  'dataConfigs',
  'releases',
]);

interface ConsoleSnapshot {
  algorithms: AlgorithmResponse[];
  auditRecords: AuditRecordResponse[];
  collectionTasks: CollectionTaskResponse[];
  dataConfigs: DataConfigResponse[];
  deviceModels: DeviceModelResponse[];
  edgeNodes: EdgeNodeResponse[];
  pointMappings: PointMappingResponse[];
  pointSets: PointSetResponse[];
  products: ProductResponse[];
  productVersions: Record<string, ProductVersionResponse[]>;
  projects: ProjectResponse[];
  protocolConnections: ProtocolConnectionResponse[];
  mqttUplink?: MqttUplinkResponse;
  discoverySuggestions: PointMappingSuggestionResponse[];
  releaseList: ReleaseListResponse;
  runtimeStatus: RuntimeStatusResponse;
  summary: SummaryResponse;
}

type AuthSession =
  | { state: 'checking' }
  | { state: 'required' }
  | { principal: AuthStatusResponse; state: 'authenticated' };

export default function App() {
  const [session, setSession] = useState<AuthSession>({ state: 'checking' });

  useEffect(() => {
    let mounted = true;
    fetchAuthStatus()
      .then((principal) => {
        if (mounted) setSession({ principal, state: 'authenticated' });
      })
      .catch(() => {
        setApiToken();
        if (mounted) setSession({ state: 'required' });
      });
    return () => {
      mounted = false;
    };
  }, []);

  const handleAuthenticate = async (token: string) => {
    setApiToken(token);
    try {
      const principal = await fetchAuthStatus();
      setSession({ principal, state: 'authenticated' });
    } catch {
      setApiToken();
      throw new Error('访问令牌无效或已失效');
    }
  };

  if (session.state === 'checking') return <AuthGate checking />;
  if (session.state === 'required') {
    return <AuthGate onAuthenticate={handleAuthenticate} />;
  }

  return (
    <ConsoleApp
      onLogout={() => {
        setApiToken();
        setSession({ state: 'required' });
      }}
      principal={session.principal}
    />
  );
}

export function ConsoleApp({
  onLogout = () => undefined,
  principal = {
    authenticationEnabled: false,
    role: 'admin',
    subject: 'local-development',
  },
}: {
  onLogout?: () => void;
  principal?: AuthStatusResponse;
}) {
  const [activePage, setActivePage] = useState<PageKey>('dashboard');
  const [summary, setSummary] = useState(initialSummary);
  const [edgeNodes, setEdgeNodes] = useState<EdgeNodeResponse[]>();
  const [deviceModels, setDeviceModels] = useState<DeviceModelResponse[]>();
  const [protocolConnections, setProtocolConnections] =
    useState<ProtocolConnectionResponse[]>();
  const [selectedProtocolEdgeId, setSelectedProtocolEdgeId] = useState('');
  const [pointMappings, setPointMappings] = useState<PointMappingResponse[]>();
  const [selectedPointEdgeId, setSelectedPointEdgeId] = useState('');
  const [collectionTasks, setCollectionTasks] = useState<CollectionTaskResponse[]>();
  const [selectedCollectionEdgeId, setSelectedCollectionEdgeId] = useState('');
  const [dataConfigs, setDataConfigs] = useState<DataConfigResponse[]>();
  const [selectedDataConfigEdgeId, setSelectedDataConfigEdgeId] = useState('');
  const [algorithms, setAlgorithms] = useState<AlgorithmResponse[]>();
  const [selectedAlgorithmEdgeId, setSelectedAlgorithmEdgeId] = useState('');
  const [mqttUplink, setMqttUplink] = useState<MqttUplinkResponse>();
  const [discoverySuggestions, setDiscoverySuggestions] =
    useState<PointMappingSuggestionResponse[]>();
  const [releaseList, setReleaseList] = useState<ReleaseListResponse>();
  const [runtimeStatus, setRuntimeStatus] = useState<RuntimeStatusResponse>();
  const [auditRecords, setAuditRecords] = useState<AuditRecordResponse[]>();
  const [edgeConfigurationMode, setEdgeConfigurationMode] =
    useState<EdgeConfigurationMode>('list');
  const [edgeConfigInitialTab, setEdgeConfigInitialTab] =
    useState<EdgeConfigTabKey>('versions');
  const [edgeTemplates, setEdgeTemplates] = useState<EdgeTemplateDefinition[]>([]);
  const [projects, setProjects] = useState<ProjectDefinition[]>([]);
  const [pointSets, setPointSets] = useState<PointSetResponse[]>([]);
  const [productVersions, setProductVersions] = useState<
    Record<string, ProductVersionResponse[]>
  >({});
  const [edgeProductBindings, setEdgeProductBindings] = useState<EdgeProductBinding[]>([]);
  const [edgeAccessTokens, setEdgeAccessTokens] = useState<EdgeAccessTokens>({});
  const [focusedRuntimeEdgeId, setFocusedRuntimeEdgeId] = useState<string>();
  const [loadState, setLoadState] = useState<'loading' | 'ready' | 'error'>(
    'loading',
  );

  const applySnapshot = (snapshot: ConsoleSnapshot) => {
    const firstEdgeId = snapshot.edgeNodes[0]?.edgeId ?? '';
    setSummary(snapshot.summary);
    setEdgeNodes(snapshot.edgeNodes);
    setEdgeProductBindings(snapshot.edgeNodes.map(edgeProductBindingFromNode));
    setDeviceModels(snapshot.deviceModels);
    setProtocolConnections(snapshot.protocolConnections);
    setMqttUplink(snapshot.mqttUplink);
    setDiscoverySuggestions(snapshot.discoverySuggestions);
    setPointMappings(snapshot.pointMappings);
    setPointSets(snapshot.pointSets);
    setProductVersions(snapshot.productVersions);
    setProjects(snapshot.projects.map(projectResponseToDefinition));
    setEdgeTemplates((current) =>
      mergeCatalogProducts(
        snapshot.products,
        snapshot.productVersions,
        snapshot.pointSets,
        current,
      ),
    );
    setCollectionTasks(snapshot.collectionTasks);
    setDataConfigs(snapshot.dataConfigs);
    setAlgorithms(snapshot.algorithms);
    setReleaseList(snapshot.releaseList);
    setRuntimeStatus(snapshot.runtimeStatus);
    setAuditRecords(snapshot.auditRecords);
    setSelectedProtocolEdgeId(firstEdgeId);
    setSelectedPointEdgeId(firstEdgeId);
    setSelectedCollectionEdgeId(firstEdgeId);
    setSelectedDataConfigEdgeId(firstEdgeId);
    setSelectedAlgorithmEdgeId(firstEdgeId);
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

  const handleCreatePoint = async (
    edgeId = defaultConfigEdgeId,
    request: CreatePointMappingRequest = {},
  ) => {
    const created = await createPointMappingDraft(edgeId, request);
    const [nextPointMappings, nextReleaseList] = await Promise.all([
      fetchEdgePointMappings(edgeId),
      fetchReleaseList(),
    ]);
    setPointMappings(nextPointMappings);
    setReleaseList(nextReleaseList);
    setSelectedPointEdgeId(edgeId);
    return created;
  };

  const handleDeletePoint = async (edgeId: string, pointId: string) => {
    await deleteEdgePointMapping(edgeId, pointId);
    const [nextPointMappings, nextReleaseList] = await Promise.all([
      fetchEdgePointMappings(edgeId),
      fetchReleaseList(),
    ]);
    setPointMappings(nextPointMappings);
    setReleaseList(nextReleaseList);
    setSelectedPointEdgeId(edgeId);
  };

  const handleValidateConfig = async (
    edgeId = defaultConfigEdgeId,
  ): Promise<ManagementActionResponse> => runConfigValidation(edgeId);

  const handleImportPoints = async (
    edgeId: string,
  ): Promise<ManagementActionResponse> => {
    const [suggestions, currentPoints, currentConnections] = await Promise.all([
      fetchDiscoverySuggestions(edgeId),
      fetchEdgePointMappings(edgeId),
      fetchEdgeProtocolConnections(edgeId),
    ]);
    const existingPointIds = new Set(currentPoints.map((point) => point.pointId));
    const knownDeviceIds = new Set(currentPoints.map((point) => point.deviceId));
    const knownConnectionIds = new Set(
      currentConnections.map((connection) => connection.connectionId),
    );
    const importableSuggestions = uniqueSuggestions(suggestions).filter(
      (suggestion) =>
        !existingPointIds.has(suggestion.pointId) &&
        knownConnectionIds.has(suggestion.protocolConnectionId) &&
        (knownDeviceIds.size === 0 || knownDeviceIds.has(suggestion.deviceId)),
    );

    if (importableSuggestions.length === 0) {
      return {
        action: 'import_points',
        details: ['没有可导入的候选点位'],
        message: '没有可导入的候选点位',
        status: '未变更',
      };
    }

    for (const suggestion of importableSuggestions) {
      await createPointMappingDraft(edgeId, suggestionToPointRequest(suggestion));
    }

    const [nextPointMappings, nextReleaseList, nextSuggestions] = await Promise.all([
      fetchEdgePointMappings(edgeId),
      fetchReleaseList(),
      fetchDiscoverySuggestions(edgeId),
    ]);
    setPointMappings(nextPointMappings);
    setReleaseList(nextReleaseList);
    setDiscoverySuggestions(nextSuggestions);
    setSelectedPointEdgeId(edgeId);

    return {
      action: 'import_points',
      details: importableSuggestions.map(
        (suggestion) =>
          `${suggestion.pointId} -> ${suggestion.protocolConnectionId}:${suggestion.address}`,
      ),
      message: `已导入 ${importableSuggestions.length} 个候选点位`,
      status: '已完成',
    };
  };

  const handleApplyEdgeTemplate = async (
    edgeId: string,
    templateId: EdgeTemplateId,
  ): Promise<ManagementActionResponse> => {
    const template = edgeTemplates.find((item) => item.templateId === templateId);
    if (!template) {
      return {
        action: 'apply_edge_template',
        details: ['产品不存在或已下线'],
        message: '产品配置加载失败',
        status: '失败',
      };
    }
    if (template.dataConfig.visualGraph?.nodes.length) {
      const graphIssues = productGraphIssueDetails(template.dataConfig.visualGraph);
      if (graphIssues.length > 0) {
        return {
          action: 'apply_edge_template',
          details: graphIssues,
          message: '采集编排存在未连接节点，请先完成拓扑',
          status: '失败',
        };
      }
    }

    const boundEdge = await bindEdgeProduct(edgeId, {
      desiredVersion: template.version,
      productId: template.templateId,
      projectId: template.projectId,
    });
    const [nextMqttUplink, nextReleaseList] = await Promise.all([
      fetchMqttUplink(edgeId),
      fetchReleaseList(),
      loadEdgeConfig(edgeId),
    ]);
    setEdgeNodes((current = []) =>
      current.map((edge) => (edge.edgeId === edgeId ? boundEdge : edge)),
    );
    setEdgeProductBindings((current) =>
      upsertEdgeProductBinding(current, edgeProductBindingFromNode(boundEdge)),
    );
    setMqttUplink(nextMqttUplink);
    setReleaseList(nextReleaseList);

    return {
      action: 'apply_edge_template',
      details: [
        `产品 ${template.name} (${template.version}) 已绑定到边端 ${edgeId}`,
        '产品版本已物化为边端配置包，等待校验与发布',
      ],
      message: `${template.name} 已加载为边端产品配置`,
      status: '已完成',
    };
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

  const handleCreateCollectionTask = async (
    edgeId: string,
    request: CreateCollectionTaskRequest,
  ) => {
    const created = await createCollectionTaskDraft(edgeId, request);
    const [nextCollectionTasks, nextReleaseList] = await Promise.all([
      fetchEdgeCollectionTasks(edgeId),
      fetchReleaseList(),
    ]);
    setCollectionTasks(nextCollectionTasks);
    setReleaseList(nextReleaseList);
    setSelectedCollectionEdgeId(edgeId);
    return created;
  };

  const handleDeleteCollectionTask = async (edgeId: string, taskId: string) => {
    await deleteEdgeCollectionTask(edgeId, taskId);
    const [nextCollectionTasks, nextReleaseList] = await Promise.all([
      fetchEdgeCollectionTasks(edgeId),
      fetchReleaseList(),
    ]);
    setCollectionTasks(nextCollectionTasks);
    setReleaseList(nextReleaseList);
    setSelectedCollectionEdgeId(edgeId);
  };

  const handleGenerateSchedule = async (
    edgeId: string,
  ): Promise<ManagementActionResponse> => {
    const result = await runConfigValidation(edgeId);
    return {
      ...result,
      action: 'generate_schedule',
      message: '调度策略已生成',
    };
  };

  const handleSelectCollectionEdge = async (edgeId: string) => {
    setSelectedCollectionEdgeId(edgeId);
    setCollectionTasks(await fetchEdgeCollectionTasks(edgeId));
  };

  const handleSaveDataConfig = async (
    edgeId: string,
    configId: string | null,
    request: SaveDataConfigRequest,
  ) => {
    if (configId) {
      await saveEdgeDataConfig(edgeId, configId, request);
    } else {
      await createEdgeDataConfig(edgeId, request);
    }
    const [nextDataConfigs, nextReleaseList] = await Promise.all([
      fetchEdgeDataConfigs(edgeId),
      fetchReleaseList(),
    ]);
    setDataConfigs(nextDataConfigs);
    setReleaseList(nextReleaseList);
    setSelectedDataConfigEdgeId(edgeId);
  };

  const handleDeleteDataConfig = async (edgeId: string, configId: string) => {
    await deleteEdgeDataConfig(edgeId, configId);
    const [nextDataConfigs, nextReleaseList] = await Promise.all([
      fetchEdgeDataConfigs(edgeId),
      fetchReleaseList(),
    ]);
    setDataConfigs(nextDataConfigs);
    setReleaseList(nextReleaseList);
    setSelectedDataConfigEdgeId(edgeId);
  };

  const handleSelectDataConfigEdge = async (edgeId: string) => {
    setSelectedDataConfigEdgeId(edgeId);
    setDataConfigs(await fetchEdgeDataConfigs(edgeId));
  };

  const handleSaveProtocolConnection = async (
    edgeId: string,
    connectionId: string,
    request: SaveProtocolConnectionRequest,
  ) => {
    await saveEdgeProtocolConnection(edgeId, connectionId, request);
    const [nextProtocolConnections, nextReleaseList] = await Promise.all([
      fetchEdgeProtocolConnections(edgeId),
      fetchReleaseList(),
    ]);
    setProtocolConnections(nextProtocolConnections);
    setReleaseList(nextReleaseList);
    setSelectedProtocolEdgeId(edgeId);
  };

  const handleCreateProtocolConnection = async (
    edgeId: string,
    request: CreateProtocolConnectionRequest,
  ) => {
    const created = await createEdgeProtocolConnection(edgeId, request);
    const [nextProtocolConnections, nextReleaseList] = await Promise.all([
      fetchEdgeProtocolConnections(edgeId),
      fetchReleaseList(),
    ]);
    setProtocolConnections(nextProtocolConnections);
    setReleaseList(nextReleaseList);
    setSelectedProtocolEdgeId(edgeId);
    return created;
  };

  const handleDeleteProtocolConnection = async (
    edgeId: string,
    connectionId: string,
  ) => {
    await deleteEdgeProtocolConnection(edgeId, connectionId);
    const [nextProtocolConnections, nextReleaseList] = await Promise.all([
      fetchEdgeProtocolConnections(edgeId),
      fetchReleaseList(),
    ]);
    setProtocolConnections(nextProtocolConnections);
    setReleaseList(nextReleaseList);
    setSelectedProtocolEdgeId(edgeId);
  };

  const handleSelectProtocolEdge = async (edgeId: string) => {
    setSelectedProtocolEdgeId(edgeId);
    setProtocolConnections(await fetchEdgeProtocolConnections(edgeId));
  };

  const handleSaveAlgorithm = async (
    edgeId: string,
    algorithmId: string,
    request: SaveAlgorithmRequest,
  ) => {
    await saveEdgeAlgorithm(edgeId, algorithmId, request);
    const [nextAlgorithms, nextReleaseList] = await Promise.all([
      fetchEdgeAlgorithms(edgeId),
      fetchReleaseList(),
    ]);
    setAlgorithms(nextAlgorithms);
    setReleaseList(nextReleaseList);
    setSelectedAlgorithmEdgeId(edgeId);
  };

  const handleCreateAlgorithm = async (
    edgeId: string,
    request: CreateAlgorithmRequest,
  ) => {
    const created = await createAlgorithmDraft(edgeId, request);
    const [nextAlgorithms, nextReleaseList] = await Promise.all([
      fetchEdgeAlgorithms(edgeId),
      fetchReleaseList(),
    ]);
    setAlgorithms(nextAlgorithms);
    setReleaseList(nextReleaseList);
    setSelectedAlgorithmEdgeId(edgeId);
    return created;
  };

  const handleDeleteAlgorithm = async (edgeId: string, algorithmId: string) => {
    await deleteEdgeAlgorithm(edgeId, algorithmId);
    const [nextAlgorithms, nextReleaseList] = await Promise.all([
      fetchEdgeAlgorithms(edgeId),
      fetchReleaseList(),
    ]);
    setAlgorithms(nextAlgorithms);
    setReleaseList(nextReleaseList);
    setSelectedAlgorithmEdgeId(edgeId);
  };

  const handleAssessAlgorithmRisk = async (
    edgeId: string,
  ): Promise<ManagementActionResponse> => {
    const result = await runConfigValidation(edgeId);
    return {
      ...result,
      action: 'assess_algorithm_risk',
      message: '算法风险评估已完成',
    };
  };

  const handleSelectAlgorithmEdge = async (edgeId: string) => {
    setSelectedAlgorithmEdgeId(edgeId);
    setAlgorithms(await fetchEdgeAlgorithms(edgeId));
  };

  const handlePublishLatestRelease = async (edgeId: string) => {
    await publishLatestRelease(edgeId);
    await refreshConsoleData();
  };

  const handleReleaseDiff = async (
    edgeId: string,
  ): Promise<ManagementActionResponse> => runReleaseDiff(edgeId);

  const handleCreateDeviceModel = async (request: CreateDeviceModelRequest) => {
    const created = await createDeviceModelDraft(request);
    setDeviceModels(await fetchDeviceModels());
    return created;
  };

  const handleSaveDeviceModel = async (
    deviceType: string,
    request: SaveDeviceModelRequest,
  ) => {
    const saved = await saveDeviceModel(deviceType, request);
    setDeviceModels(await fetchDeviceModels());
    return saved;
  };

  const handleDeleteDeviceModel = async (deviceType: string) => {
    await deleteDeviceModel(deviceType);
    setDeviceModels(await fetchDeviceModels());
    setReleaseList(await fetchReleaseList());
  };

  const handleDeleteEdgeNode = async (edgeId: string) => {
    await deleteEdgeNode(edgeId);
    const [nextEdges, nextSummary, nextReleaseList] = await Promise.all([
      fetchEdgeNodes(),
      fetchSummary(),
      fetchReleaseList(),
    ]);
    setEdgeNodes(nextEdges);
    setSummary(nextSummary);
    setReleaseList(nextReleaseList);
    const nextEdgeId = nextEdges[0]?.edgeId;
    if (nextEdgeId) {
      await loadEdgeConfig(nextEdgeId);
    } else {
      setSelectedProtocolEdgeId('');
      setSelectedPointEdgeId('');
      setSelectedCollectionEdgeId('');
      setSelectedDataConfigEdgeId('');
      setSelectedAlgorithmEdgeId('');
      setProtocolConnections([]);
      setPointMappings([]);
      setCollectionTasks([]);
      setDataConfigs([]);
      setAlgorithms([]);
      setMqttUplink(undefined);
      setDiscoverySuggestions([]);
    }
  };

  const handleCreateManagedEdge = async (
    request: CreateManagedEdgeRequest,
  ): Promise<EdgeNodeResponse> => {
    const created = await createEdgeNode({
      displayName: request.displayName,
      productId: request.productId,
      projectId: request.projectId,
      site: request.site,
    });

    setEdgeNodes((current = []) => [created, ...current]);
    setSelectedProtocolEdgeId(created.edgeId);
    setSelectedPointEdgeId(created.edgeId);
    setSelectedCollectionEdgeId(created.edgeId);
    setSelectedDataConfigEdgeId(created.edgeId);
    setSelectedAlgorithmEdgeId(created.edgeId);
    if (created.accessToken) {
      setEdgeAccessTokens((current) => ({
        ...current,
        [created.edgeId]: created.accessToken as string,
      }));
    }
    setEdgeProductBindings((current) =>
      upsertEdgeProductBinding(current, edgeProductBindingFromNode(created)),
    );
    setSummary((current) => ({
      ...current,
      edge_count: (current.edge_count || 0) + 1,
    }));
    setReleaseList(await fetchReleaseList());
    return created;
  };

  const handleGenerateEdgeAccessToken = async (edgeId: string) => {
    const generated = await generateEdgeAccessTokenApi(edgeId);
    setEdgeAccessTokens((current) => ({
      ...current,
      [edgeId]: generated.accessToken,
    }));
    return generated.accessToken;
  };

  const handleAgentSafetyCheck = async (): Promise<AgentActionResponse> =>
    runAgentSafetyCheck();

  const handleGenerateAgentSuggestions = async (): Promise<AgentActionResponse> =>
    generateAgentSuggestions();

  const handleAgentChat = async (request: AgentChatRequest): Promise<AgentChatResponse> =>
    sendAgentChat(request);

  const handleAgentProviderStatus = async (): Promise<AgentProviderStatusResponse> =>
    fetchAgentProviderStatus();

  const handleListAgentConversations = async (
    projectId?: string,
  ): Promise<AgentConversationResponse[]> =>
    fetchAgentConversations(principal.subject, projectId);

  const handleDeleteAgentConversation = async (conversationId: string): Promise<void> =>
    deleteAgentConversation(conversationId);

  const handleListAgentKnowledge = async (
    projectId?: string,
  ): Promise<AgentKnowledgeDocumentResponse[]> =>
    fetchAgentKnowledgeDocuments(projectId);

  const handleSaveAgentKnowledge = async (
    documentId: string | null,
    request: SaveAgentKnowledgeDocumentRequest,
  ): Promise<AgentKnowledgeDocumentResponse> =>
    documentId
      ? saveAgentKnowledgeDocument(documentId, request)
      : createAgentKnowledgeDocument(request);

  const handleDeleteAgentKnowledge = async (documentId: string): Promise<void> =>
    deleteAgentKnowledgeDocument(documentId);

  const handleCreateAgentProposal = async (
    request: CreateAgentProposalRequest,
  ): Promise<AgentProposalResponse> => createAgentProposal(request);

  const handleReviewAgentProposal = async (
    proposalId: string,
    decision: 'approve' | 'reject',
    request: ReviewAgentProposalRequest,
  ): Promise<AgentProposalResponse> =>
    reviewAgentProposal(proposalId, decision, request);

  const handleSaveMqttUplink = async (
    edgeId: string,
    request: MqttUplinkResponse,
  ) => {
    const saved = await saveMqttUplink(edgeId, request);
    setMqttUplink(saved);
    setReleaseList(await fetchReleaseList());
    return saved;
  };

  const handleCreateProject = async () => {
    const sequence = nextCatalogSequence(
      projects.map((project) => project.projectId),
      'project-',
    );
    const request: SaveProjectRequest = {
      description: '用于承载一组产品配置与边端实例的项目空间',
      environment: 'staging',
      name: `新项目 ${sequence}`,
      owner: 'platform-team',
      projectId: `project-${sequence}`,
    };
    const nextProject = projectResponseToDefinition(await createProjectApi(request));
    setProjects((current) => [nextProject, ...current]);
    return nextProject;
  };

  const handleSaveProject = async (
    projectId: string,
    nextProject: ProjectDefinition,
  ) => {
    const savedProject = projectResponseToDefinition(
      await saveProjectApi(projectId, projectDefinitionToRequest(nextProject)),
    );
    setProjects((current) =>
      current.map((project) =>
        project.projectId === projectId ? savedProject : project,
      ),
    );
    return savedProject;
  };

  const handleDeleteProject = async (projectId: string) => {
    await deleteProjectApi(projectId);
    setProjects((current) => current.filter((project) => project.projectId !== projectId));
  };

  const handleCreatePointSet = async (request: SavePointSetRequest) => {
    const created = await createPointSetApi(request);
    setPointSets((current) => [created, ...current]);
    return created;
  };

  const handleSavePointSet = async (
    pointSetId: string,
    request: SavePointSetRequest,
  ) => {
    const saved = await savePointSetApi(pointSetId, request);
    setPointSets((current) =>
      current.map((pointSet) =>
        pointSet.pointSetId === pointSetId ? saved : pointSet,
      ),
    );
    return saved;
  };

  const handleDeletePointSet = async (pointSetId: string) => {
    await deletePointSetApi(pointSetId);
    setPointSets((current) =>
      current.filter((pointSet) => pointSet.pointSetId !== pointSetId),
    );
  };

  const handleRunDiscovery = async (
    edgeId: string,
    request: RunDiscoveryRequest,
  ): Promise<DiscoveryReportResponse> => {
    const report = await runDiscovery(edgeId, request);
    setDiscoverySuggestions(report.suggestions);
    return report;
  };

  const handleCreateEdgeTemplate = async () => {
    if (projects.length === 0) {
      throw new Error('请先创建项目，再在项目下创建产品');
    }
    const sequence = edgeTemplates.length + 1;
    const source = edgeTemplates.find(
      (template) => template.templateId === DEFAULT_EDGE_TEMPLATE_ID,
    ) ?? edgeTemplates[0] ?? EDGE_CONFIG_TEMPLATES[0];
    const nextTemplate: EdgeTemplateDefinition = {
      ...source,
      dataConfig: {
        ...source.dataConfig,
        configId: `product_data_${sequence}`,
        name: `自定义产品上报 ${sequence}`,
      },
      name: `自定义边端产品 ${sequence}`,
      pointSetIds: [],
      productType: source.productType,
      projectId: projects[0].projectId,
      templateId: `custom-product-${Date.now()}`,
      version: 'v1.0.0',
    };
    const productRequest: SaveProductRequest = {
      description: nextTemplate.description,
      name: nextTemplate.name,
      productId: nextTemplate.templateId,
      productType: nextTemplate.productType,
      projectId: nextTemplate.projectId,
    };
    await createProductApi(productRequest);
    let version: ProductVersionResponse;
    try {
      version = await createProductVersion(
        nextTemplate.templateId,
        buildProductVersionRequest(nextTemplate, nextTemplate.version, pointSets),
      );
    } catch (error) {
      await deleteProductApi(nextTemplate.templateId).catch(() => undefined);
      throw error;
    }
    nextTemplate.version = version.version;
    setProductVersions((current) => ({
      ...current,
      [nextTemplate.templateId]: [version],
    }));
    setEdgeTemplates((current) => [nextTemplate, ...current]);
    return nextTemplate;
  };

  const handleSaveEdgeTemplate = async (
    templateId: EdgeTemplateId,
    nextTemplate: EdgeTemplateDefinition,
  ) => {
    await saveProductApi(templateId, {
      description: nextTemplate.description,
      name: nextTemplate.name,
      productId: templateId,
      productType: nextTemplate.productType,
      projectId: nextTemplate.projectId,
    });
    const versions = productVersions[templateId] ?? [];
    const draft = versions.find((version) => version.status === 'draft');
    const targetVersion = draft?.version ?? nextProductVersion(nextTemplate.version);
    const request = buildProductVersionRequest(nextTemplate, targetVersion, pointSets);
    const savedVersion = draft
      ? await saveProductVersion(templateId, targetVersion, request)
      : await createProductVersion(templateId, request);
    const savedTemplate = { ...nextTemplate, templateId, version: savedVersion.version };
    setProductVersions((current) => ({
      ...current,
      [templateId]: draft
        ? (current[templateId] ?? []).map((version) =>
            version.version === savedVersion.version ? savedVersion : version,
          )
        : [savedVersion, ...(current[templateId] ?? [])],
    }));
    setEdgeTemplates((current) =>
      current.map((template) =>
        template.templateId === templateId ? savedTemplate : template,
      ),
    );
    return savedTemplate;
  };

  const refreshProductLifecycle = async (productId: string) => {
    const [products, versions] = await Promise.all([
      fetchProducts(),
      fetchProductVersions(productId),
    ]);
    const nextVersions = { ...productVersions, [productId]: versions };
    setProductVersions(nextVersions);
    setEdgeTemplates((current) =>
      mergeCatalogProducts(products, nextVersions, pointSets, current),
    );
    return versions;
  };

  const refreshProductRollout = async (productId: string) => {
    await refreshProductLifecycle(productId);
    const [nextEdges, nextSummary, nextReleaseList] = await Promise.all([
      fetchEdgeNodes(),
      fetchSummary(),
      fetchReleaseList(),
    ]);
    setEdgeNodes(nextEdges);
    setEdgeProductBindings(nextEdges.map(edgeProductBindingFromNode));
    setSummary(nextSummary);
    setReleaseList(nextReleaseList);
  };

  const handlePublishProductVersion = async (
    productId: string,
    version: string,
  ) => {
    const published = await publishProductVersionApi(productId, version);
    await refreshProductRollout(productId);
    return published;
  };

  const handleRollbackProductVersion = async (
    productId: string,
    version: string,
  ) => {
    const published = await rollbackProductVersionApi(productId, version);
    await refreshProductRollout(productId);
    return published;
  };

  const handleDeleteEdgeTemplate = async (templateId: EdgeTemplateId) => {
    await deleteProductApi(templateId);
    setEdgeTemplates((current) =>
      current.filter((template) => template.templateId !== templateId),
    );
    setProductVersions((current) => {
      const next = { ...current };
      delete next[templateId];
      return next;
    });
  };

  const handleNavigate = (page: PageKey) => {
    setActivePage(page);
    setEdgeConfigurationMode(configurationPages.has(page) ? 'configure' : 'list');
    if (page !== 'runtimeStatus') {
      setFocusedRuntimeEdgeId(undefined);
    }
  };

  const loadEdgeConfig = async (edgeId: string) => {
    setSelectedProtocolEdgeId(edgeId);
    setSelectedPointEdgeId(edgeId);
    setSelectedCollectionEdgeId(edgeId);
    setSelectedDataConfigEdgeId(edgeId);
    setSelectedAlgorithmEdgeId(edgeId);

    const [
      nextProtocolConnections,
      nextPointMappings,
      nextCollectionTasks,
      nextDataConfigs,
      nextAlgorithms,
      nextMqttUplink,
      nextDiscoverySuggestions,
    ] = await Promise.all([
      fetchEdgeProtocolConnections(edgeId),
      fetchEdgePointMappings(edgeId),
      fetchEdgeCollectionTasks(edgeId),
      fetchEdgeDataConfigs(edgeId),
      fetchEdgeAlgorithms(edgeId),
      fetchMqttUplink(edgeId),
      fetchDiscoverySuggestions(edgeId),
    ]);
    setProtocolConnections(nextProtocolConnections);
    setPointMappings(nextPointMappings);
    setCollectionTasks(nextCollectionTasks);
    setDataConfigs(nextDataConfigs);
    setAlgorithms(nextAlgorithms);
    setMqttUplink(nextMqttUplink);
    setDiscoverySuggestions(nextDiscoverySuggestions);
  };

  const handleMonitorEdge = (edgeId: string) => {
    setFocusedRuntimeEdgeId(edgeId);
    setActivePage('runtimeStatus');
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
    if (deprecatedGlobalConfigurationPages.has(activePage)) {
      setEdgeConfigurationMode('list');
      setActivePage('edges');
    }
  }, [activePage]);

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
    <AppShell
      activePage={activePage}
      onNavigate={handleNavigate}
      onLogout={onLogout}
      platformStatus={{
        environment: projects[0]?.environment ?? '未配置',
        onlineEdgeCount:
          runtimeStatus?.healthyEdgeCount ??
          edgeNodes?.filter((edge) => edge.status === '健康').length ??
          0,
        pendingReleaseCount: summary.pending_release_count,
        project: projects[0]?.projectName ?? '暂无项目',
      }}
      principal={principal}
    >
      {renderPage(
        activePage,
        summary,
        loadState,
        edgeConfigurationMode,
        focusedRuntimeEdgeId,
        edgeConfigInitialTab,
        edgeTemplates,
        projects,
        edgeProductBindings,
        edgeAccessTokens,
        handleCreateProject,
        handleSaveProject,
        handleDeleteProject,
        pointSets,
        handleCreatePointSet,
        handleSavePointSet,
        handleDeletePointSet,
        handleCreateEdgeTemplate,
        handleSaveEdgeTemplate,
        handleDeleteEdgeTemplate,
        productVersions,
        handlePublishProductVersion,
        handleRollbackProductVersion,
        handleAgentSafetyCheck,
        handleAssessAlgorithmRisk,
        loadEdgeConfig,
        handleCreateManagedEdge,
        handleGenerateEdgeAccessToken,
        handleDeleteEdgeNode,
        handleCreateAlgorithm,
        handleSaveAlgorithm,
        handleDeleteAlgorithm,
        handleCreateCollectionTask,
        handleDeleteCollectionTask,
        handleCreateDeviceModel,
        handleSaveDeviceModel,
        handleDeleteDeviceModel,
        handleCreatePoint,
        handleDeletePoint,
        handleGenerateAgentSuggestions,
        handleAgentChat,
        handleAgentProviderStatus,
        handleListAgentConversations,
        handleDeleteAgentConversation,
        handleListAgentKnowledge,
        handleSaveAgentKnowledge,
        handleDeleteAgentKnowledge,
        handleCreateAgentProposal,
        fetchAgentProposals,
        handleReviewAgentProposal,
        handleRunDiscovery,
        handleGenerateSchedule,
        handleImportPoints,
        handleApplyEdgeTemplate,
        handleMonitorEdge,
        handleReleaseDiff,
        handleSavePoint,
        handleSelectPointEdge,
        selectedPointEdgeId,
        handleSaveCollectionTask,
        handleSelectCollectionEdge,
        selectedCollectionEdgeId,
        handleSaveDataConfig,
        handleDeleteDataConfig,
        selectedDataConfigEdgeId,
        handleSaveProtocolConnection,
        handleCreateProtocolConnection,
        handleDeleteProtocolConnection,
        selectedProtocolEdgeId,
        handleSaveMqttUplink,
        handlePublishLatestRelease,
        handleValidateConfig,
        edgeNodes,
        deviceModels,
        protocolConnections,
        mqttUplink,
        discoverySuggestions,
        pointMappings,
        collectionTasks,
        dataConfigs,
        algorithms,
        releaseList,
        runtimeStatus,
        auditRecords,
        principal,
      )}
    </AppShell>
  );
}

async function loadConsoleSnapshot(): Promise<ConsoleSnapshot> {
  const [
    summary,
    edgeNodes,
    deviceModels,
    pointSets,
    products,
    projects,
    releaseList,
    runtimeStatus,
    auditRecords,
  ] = await Promise.all([
    fetchSummary(),
    fetchEdgeNodes(),
    fetchDeviceModels(),
    fetchPointSets(),
    fetchProducts(),
    fetchProjects(),
    fetchReleaseList(),
    fetchRuntimeStatus(),
    fetchAuditRecords(),
  ]);

  const selectedEdgeId = edgeNodes[0]?.edgeId;
  const [
    protocolConnections,
    mqttUplink,
    discoverySuggestions,
    pointMappings,
    collectionTasks,
    dataConfigs,
    algorithms,
  ] = selectedEdgeId
    ? await Promise.all([
        fetchEdgeProtocolConnections(selectedEdgeId),
        fetchMqttUplink(selectedEdgeId),
        fetchDiscoverySuggestions(selectedEdgeId),
        fetchEdgePointMappings(selectedEdgeId),
        fetchEdgeCollectionTasks(selectedEdgeId),
        fetchEdgeDataConfigs(selectedEdgeId),
        fetchEdgeAlgorithms(selectedEdgeId),
      ])
    : [[], undefined, [], [], [], [], []];

  const productVersions = Object.fromEntries(
    await Promise.all(
      products.map(async (product) => [
        product.productId,
        await fetchProductVersions(product.productId),
      ] as const),
    ),
  );

  return {
    algorithms,
    auditRecords,
    collectionTasks,
    dataConfigs,
    deviceModels,
    edgeNodes,
    pointMappings,
    pointSets,
    products,
    productVersions,
    projects,
    protocolConnections,
    mqttUplink,
    discoverySuggestions,
    releaseList,
    runtimeStatus,
    summary,
  };
}

function projectResponseToDefinition(project: ProjectResponse): ProjectDefinition {
  return {
    description: project.description,
    environment: project.environment,
    owner: project.owner,
    projectId: project.projectId,
    projectName: project.name,
  };
}

function projectDefinitionToRequest(project: ProjectDefinition): SaveProjectRequest {
  return {
    description: project.description,
    environment: project.environment,
    name: project.projectName,
    owner: project.owner,
    projectId: project.projectId,
  };
}

function mergeCatalogProducts(
  products: ProductResponse[],
  versions: Record<string, ProductVersionResponse[]>,
  pointSets: PointSetResponse[],
  current: EdgeTemplateDefinition[],
): EdgeTemplateDefinition[] {
  if (products.length === 0) return [];
  const fallback =
    current.find((template) => template.templateId === DEFAULT_EDGE_TEMPLATE_ID) ??
    current[0] ??
    EDGE_CONFIG_TEMPLATES[0];

  return products.map((product) => {
    const configured = current.find(
      (template) => template.templateId === product.productId,
    );
    const base = {
      ...(configured ?? fallback),
      description: product.description,
      name: product.name,
      productType: product.productType,
      projectId: product.projectId,
      templateId: product.productId,
      version: product.latestVersion ?? configured?.version ?? 'v1.0.0',
    };
    const candidates = versions[product.productId] ?? [];
    const version =
      candidates.find((candidate) => candidate.status === 'draft') ??
      candidates.find((candidate) => candidate.version === product.latestVersion) ??
      candidates[0];
    return version
      ? hydrateProductTemplate(base, version, pointSets)
      : base;
  });
}

interface CorePointAddress {
  kind: string;
  value: string;
}

interface CoreDataConfigPoint {
  address: CorePointAddress;
  json_field: string;
  point_id: string;
  semantic_id: string;
  unit?: string | null;
  value_type: string;
}

interface CoreProductAlgorithmSpec {
  dsl?: CreateAlgorithmRequest['dsl'];
  id?: string;
  kind?: CreateAlgorithmRequest['algorithmKind'];
  version?: string;
}

export function hydrateProductTemplate(
  base: EdgeTemplateDefinition,
  version: ProductVersionResponse,
  pointSets: PointSetResponse[],
): EdgeTemplateDefinition {
  const connection = version.protocolConnections[0] as
    | { endpoint?: string | null; protocol?: string }
    | undefined;
  const task = version.collectionTasks[0] as
    | {
        device_id?: string;
        enabled?: boolean;
        interval_ms?: number;
        point_ids?: string[];
        task_id?: string;
      }
    | undefined;
  const dataConfig = version.dataConfigs[0] as
    | {
        algorithm_ids?: string[];
        collection?: { period_ms?: number; retry_count?: number; timeout_ms?: number };
        config_id?: string;
        device_id?: string;
        enabled?: boolean;
        name?: string;
        points?: CoreDataConfigPoint[];
        publish?: {
          payload?: { include_quality?: boolean; mode?: string; timestamp_field?: string };
          qos?: number;
          sink_id?: string;
          topic_template?: string;
        };
        visual_graph?: {
          edges?: Array<{
            edge_id: string;
            from: string;
            from_port?: string | null;
            to: string;
            to_port?: string | null;
          }>;
          nodes?: Array<{
            kind: string;
            label: string;
            node_id: string;
            params?: Record<string, boolean | number | string | string[]>;
            ref_id?: string | null;
            x: number;
            y: number;
          }>;
        };
      }
    | undefined;
  const algorithms = version.algorithms as CoreProductAlgorithmSpec[];
  const algorithm = algorithms[0];
  const algorithmKindsById = new Map(
    algorithms.flatMap((candidate) =>
      candidate.id ? [[candidate.id, productComputeKindFromStoredAlgorithm(candidate)] as const] : [],
    ),
  );
  const mqtt = version.mqttUplinks[0] as
    | {
        batch_size?: number;
        broker?: string;
        client_id?: string;
        flush_interval_ms?: number;
        qos?: number;
        sink_id?: string;
        topic_template?: string;
      }
    | undefined;

  const pointSetPoints = pointSets
    .filter((pointSet) => version.pointSetIds.includes(pointSet.pointSetId))
    .flatMap((pointSet) => pointSet.points);
  const rawDataPoints = dataConfig?.points ?? [];
  const deviceId = task?.device_id ?? dataConfig?.device_id ?? base.task.deviceId;
  const points = (pointSetPoints.length > 0 ? pointSetPoints : rawDataPoints).map(
    (point) => ({
      addressKind: point.address.kind,
      addressValue: point.address.value,
      deviceId,
      intervalMs:
        'intervalMs' in point
          ? point.intervalMs
          : task?.interval_ms ?? dataConfig?.collection?.period_ms ?? 1000,
      pointId: 'pointId' in point ? point.pointId : point.point_id,
      semanticId: 'semanticId' in point ? point.semanticId : point.semantic_id,
      unit: point.unit ?? '',
      valueType: coreTelemetryTypeToConsole(
        'valueType' in point ? point.valueType : point.value_type,
      ),
    }),
  );
  const dataPoints: DataConfigPoint[] = rawDataPoints.map((point) => ({
    addressKind: point.address.kind,
    addressValue: point.address.value,
    jsonField: point.json_field,
    pointId: point.point_id,
    semanticId: point.semantic_id,
    unit: point.unit ?? '',
    valueType: coreTelemetryTypeToConsole(point.value_type),
  }));

  return {
    ...base,
    algorithm:
      algorithm?.id && algorithm.dsl && algorithm.kind
        ? {
            algorithmId: algorithm.id,
            algorithmKind: algorithm.kind,
            dsl: algorithm.dsl,
            version: algorithm.version ?? version.version,
          }
        : base.algorithm,
    connection: {
      endpoint: connection?.endpoint ?? base.connection.endpoint ?? null,
      protocolType: connection?.protocol ?? base.connection.protocolType,
    },
    dataConfig: dataConfig
      ? {
          algorithmIds: dataConfig.algorithm_ids ?? [],
          collection: {
            periodMs: dataConfig.collection?.period_ms ?? 1000,
            retryCount: dataConfig.collection?.retry_count ?? 2,
            timeoutMs: dataConfig.collection?.timeout_ms ?? 800,
          },
          configId: dataConfig.config_id ?? base.dataConfig.configId,
          deviceId,
          enabled: dataConfig.enabled ?? true,
          name: dataConfig.name ?? base.dataConfig.name,
          points: dataPoints,
          publish: {
            payload: {
              includeQuality: dataConfig.publish?.payload?.include_quality ?? true,
              mode:
                dataConfig.publish?.payload?.mode === 'Array' ? 'array' : 'object',
              timestampField: dataConfig.publish?.payload?.timestamp_field ?? 'ts',
            },
            qos: dataConfig.publish?.qos ?? 1,
            sinkId: dataConfig.publish?.sink_id ?? 'velamq-main',
            topicTemplate:
              dataConfig.publish?.topic_template ?? base.dataConfig.publish.topicTemplate,
          },
          visualGraph: {
            edges: (dataConfig.visual_graph?.edges ?? []).map((edge) => ({
              edgeId: edge.edge_id,
              from: edge.from,
              fromPort: edge.from_port,
              to: edge.to,
              toPort: edge.to_port,
            })),
            nodes: (dataConfig.visual_graph?.nodes ?? []).map((node) => ({
              kind: node.kind.toLowerCase() as DataConfigVisualGraph['nodes'][number]['kind'],
              label: node.label,
              nodeId: node.node_id,
              params: node.params ?? {},
              refId:
                node.kind.toLowerCase() === 'algorithm' && node.ref_id
                  ? algorithmKindsById.get(node.ref_id) ?? node.ref_id
                  : node.ref_id,
              x: node.x,
              y: node.y,
            })),
          },
        }
      : base.dataConfig,
    mqtt: mqtt
      ? {
          batchSize: mqtt.batch_size ?? 100,
          broker: mqtt.broker ?? base.mqtt.broker,
          clientId: mqtt.client_id ?? base.mqtt.clientId,
          flushIntervalMs: mqtt.flush_interval_ms ?? 1000,
          qos: mqtt.qos ?? 1,
          sinkId: mqtt.sink_id ?? 'velamq-main',
          topicTemplate: mqtt.topic_template ?? base.mqtt.topicTemplate,
        }
      : base.mqtt,
    pointSetIds: [...version.pointSetIds],
    points: points.length > 0 ? points : base.points,
    task: {
      deviceId,
      enabled: task?.enabled ?? true,
      intervalMs: task?.interval_ms ?? dataConfig?.collection?.period_ms ?? 1000,
      pointIds: task?.point_ids ?? points.map((point) => point.pointId),
      taskId: task?.task_id ?? base.task.taskId,
    },
    version: version.version,
  };
}

function productComputeKindFromStoredAlgorithm(algorithm: CoreProductAlgorithmSpec) {
  const step = algorithm.dsl?.steps[0];
  switch (algorithm.kind) {
    case 'WindowAggregate': {
      if (step?.type === 'windowAggregate' && step.functions.length === 1 && step.functions[0]?.function === 'avg') {
        return 'moving_average';
      }
      return 'window_aggregate';
    }
    case 'Statistics':
      return 'statistics';
    case 'ThresholdRule':
      return step?.type === 'conditionalRoute' ? 'condition_route' : 'alarm_event';
    case 'Deadband':
      return 'deadband_filter';
    case 'ChangeReport':
      return 'change_report';
    case 'Debounce':
      return 'debounce';
    case 'DurationRule':
      return 'duration_condition';
    case 'ExpressionAggregate': {
      if (step?.type === 'scale') return 'scale_offset';
      if (step?.type === 'clamp') return 'clamp';
      if (step?.type === 'rateOfChange') return 'rate_of_change';
      return 'expression';
    }
    default:
      return 'expression';
  }
}

function coreTelemetryTypeToConsole(valueType: string) {
  switch (valueType) {
    case 'Boolean':
      return 'bool';
    case 'Integer':
      return 'int64';
    case 'String':
      return 'string';
    default:
      return 'float32';
  }
}

function nextCatalogSequence(ids: string[], prefix: string) {
  return (
    Math.max(
      0,
      ...ids.map((id) => {
        if (!id.startsWith(prefix)) return 0;
        const parsed = Number.parseInt(id.slice(prefix.length), 10);
        return Number.isFinite(parsed) ? parsed : 0;
      }),
    ) + 1
  );
}

function nextProductVersion(version: string) {
  const match = version.match(/^v?(\d+)\.(\d+)\.(\d+)/);
  if (!match) return `v1.0.${Date.now()}`;
  return `v${match[1]}.${match[2]}.${Number.parseInt(match[3], 10) + 1}`;
}

export function buildProductVersionRequest(
  template: EdgeTemplateDefinition,
  version: string,
  pointSets: PointSetResponse[],
): SaveProductVersionRequest {
  const connectionId = `${template.templateId}-connection`;
  const runtime = materializeProductRuntime(template, connectionId);
  const pointSetIds = template.pointSetIds
    ? pointSets
        .filter(
          (pointSet) =>
            pointSet.projectId === template.projectId &&
            template.pointSetIds?.includes(pointSet.pointSetId),
        )
        .map((pointSet) => pointSet.pointSetId)
    : pointSets
        .filter(
          (pointSet) =>
            pointSet.projectId === template.projectId &&
            pointSet.points.length > 0 &&
            pointSet.points.every((point) =>
              template.points.some((candidate) => candidate.pointId === point.pointId),
            ),
        )
        .map((pointSet) => pointSet.pointSetId);

  return {
    algorithms: runtime.algorithms.map((algorithm) => ({
      dsl: algorithm.dsl,
      id: algorithm.algorithmId,
      inputs: algorithm.dsl.inputs.map((input) => input.pointId),
      kind: algorithm.algorithmKind,
      outputs: algorithm.dsl.outputs.map((output) => output.pointId),
      runtime: 'Rule',
      version: algorithm.version,
    })),
    collectionTasks: [
      {
        device_id: template.task.deviceId,
        enabled: template.task.enabled ?? true,
        interval_ms: template.task.intervalMs,
        point_ids: template.task.pointIds,
        task_id: template.task.taskId,
      },
    ],
    dataConfigs: [
      {
        algorithm_ids: runtime.dataConfig.algorithmIds ?? [],
        collection: {
          period_ms: runtime.dataConfig.collection.periodMs,
          retry_count: runtime.dataConfig.collection.retryCount,
          timeout_ms: runtime.dataConfig.collection.timeoutMs,
        },
        config_id: runtime.dataConfig.configId,
        device_id: runtime.dataConfig.deviceId,
        enabled: runtime.dataConfig.enabled,
        name: runtime.dataConfig.name,
        points: runtime.dataConfig.points.map((point) => ({
          address: { kind: point.addressKind, value: point.addressValue },
          json_field: point.jsonField,
          point_id: point.pointId,
          semantic_id: point.semanticId,
          unit: point.unit || null,
          value_type: telemetryTypeToCore(point.valueType),
        })),
        protocol_connection_id: connectionId,
        publish: {
          payload: {
            include_quality: runtime.dataConfig.publish.payload.includeQuality,
            mode:
              runtime.dataConfig.publish.payload.mode === 'array' ? 'Array' : 'Object',
            timestamp_field: runtime.dataConfig.publish.payload.timestampField,
          },
          qos: runtime.dataConfig.publish.qos,
          sink_id: runtime.dataConfig.publish.sinkId,
          topic_template: runtime.dataConfig.publish.topicTemplate,
        },
        visual_graph: {
          edges: (runtime.dataConfig.visualGraph?.edges ?? []).map((edge) => ({
            edge_id: edge.edgeId,
            from: edge.from,
            from_port: edge.fromPort ?? null,
            to: edge.to,
            to_port: edge.toPort ?? null,
          })),
          nodes: (runtime.dataConfig.visualGraph?.nodes ?? []).map((node) => ({
            kind:
              node.kind === 'point'
                ? 'Point'
                : node.kind === 'algorithm'
                  ? 'Algorithm'
                  : node.kind === 'mqtt'
                    ? 'Mqtt'
                    : 'Json',
            label: node.label,
            node_id: node.nodeId,
            params: node.params ?? {},
            ref_id: node.refId ?? null,
            x: Math.round(node.x),
            y: Math.round(node.y),
          })),
        },
      },
    ],
    deviceModels: [],
    devices: [
      {
        device_id: template.task.deviceId,
        device_type: template.productType,
      },
    ],
    mqttUplinks: [
      {
        batch_size: template.mqtt.batchSize,
        broker: template.mqtt.broker,
        client_id: template.mqtt.clientId,
        flush_interval_ms: template.mqtt.flushIntervalMs,
        qos: template.mqtt.qos,
        sink_id: template.mqtt.sinkId,
        topic_template: template.mqtt.topicTemplate,
      },
    ],
    pointSetIds,
    protocolConnections: [
      {
        connection_id: connectionId,
        endpoint: template.connection.endpoint,
        protocol: template.connection.protocolType,
      },
    ],
    version,
  };
}

function telemetryTypeToCore(valueType: string) {
  switch (valueType.toLowerCase()) {
    case 'bool':
    case 'boolean':
      return 'Boolean';
    case 'int32':
    case 'int64':
    case 'integer':
      return 'Integer';
    case 'string':
      return 'String';
    default:
      return 'Float';
  }
}

function suggestionToPointRequest(
  suggestion: PointMappingSuggestionResponse,
): CreatePointMappingRequest {
  const [addressKind, ...addressParts] = suggestion.address.split(':');
  return {
    addressKind: addressKind || 'simulated',
    addressValue: addressParts.join(':') || suggestion.pointId,
    connectionId: suggestion.protocolConnectionId,
    deviceId: suggestion.deviceId,
    pointId: suggestion.pointId,
    semanticId: suggestion.semanticId,
    unit: suggestion.unit,
    valueType: suggestion.valueType,
  };
}

function uniqueSuggestions(suggestions: PointMappingSuggestionResponse[]) {
  const seen = new Set<string>();
  return suggestions.filter((suggestion) => {
    if (seen.has(suggestion.pointId)) {
      return false;
    }
    seen.add(suggestion.pointId);
    return true;
  });
}

export const EDGE_CONFIG_TEMPLATES: EdgeTemplateDefinition[] = [
  {
    algorithm: {
      algorithmId: 'meter-window-1m',
      algorithmKind: 'WindowAggregate',
      dsl: {
        inputs: [
          { alias: 'ua', pointId: 'meter_voltage_a' },
          { alias: 'ia', pointId: 'meter_current_a' },
        ],
        report: { mode: 'WindowResult', sink: 'velamq-main' },
        steps: [
          {
            type: 'windowAggregate',
            source: 'ua',
            functions: [{ function: 'avg', output: 'voltage_a_avg_1m' }],
          },
        ],
        outputs: [{ name: 'voltage_a_avg_1m', pointId: 'meter_voltage_a.avg_1m' }],
        trigger: { type: 'window', everyMs: 60000 },
      },
      version: '1.0.0',
    },
    connection: { endpoint: '/dev/ttyUSB0', protocolType: 'ModbusRtu' },
    dataConfig: {
      collection: { periodMs: 1000, retryCount: 2, timeoutMs: 800 },
      configId: 'meter_realtime_telemetry',
      deviceId: 'meter-1',
      enabled: true,
      name: '电表实时遥测上报',
      points: [
        {
          addressKind: 'holding_register',
          addressValue: '40001',
          jsonField: 'voltage_a',
          pointId: 'meter_voltage_a',
          semanticId: 'electric.voltage_a',
          unit: 'V',
          valueType: 'float32',
        },
        {
          addressKind: 'holding_register',
          addressValue: '40003',
          jsonField: 'current_a',
          pointId: 'meter_current_a',
          semanticId: 'electric.current_a',
          unit: 'A',
          valueType: 'float32',
        },
      ],
      publish: {
        payload: { includeQuality: true, mode: 'object', timestampField: 'ts' },
        qos: 1,
        sinkId: 'velamq-main',
        topicTemplate: 'factory/{edge_id}/meter/{device_id}/telemetry',
      },
    },
    description:
      '为常见 RS485 电表初始化 Modbus RTU 连接、A 相电压/电流点位、1 秒采集任务、窗口聚合算法和 velaMQ 上报流水线。',
    highlights: ['串口 Modbus RTU', '1 秒周期采集', '窗口聚合', 'MQTT JSON 上报'],
    mqtt: {
      batchSize: 100,
      broker: 'mqtts://velamq.local:8883',
      clientId: '{edge_id}-runtime',
      flushIntervalMs: 1000,
      qos: 1,
      sinkId: 'velamq-main',
      topicTemplate: 'factory/{edge_id}/{device_id}/telemetry',
    },
    name: 'Modbus 电表标准模板',
    points: [
      {
        addressKind: 'holding_register',
        addressValue: '40001',
        deviceId: 'meter-1',
        intervalMs: 1000,
        pointId: 'meter_voltage_a',
        semanticId: 'electric.voltage_a',
        unit: 'V',
        valueType: 'float32',
      },
      {
        addressKind: 'holding_register',
        addressValue: '40003',
        deviceId: 'meter-1',
        intervalMs: 1000,
        pointId: 'meter_current_a',
        semanticId: 'electric.current_a',
        unit: 'A',
        valueType: 'float32',
      },
    ],
    productType: 'meter',
    projectId: 'demo-plant',
    recommendedFor: 'RS485 电表、配电柜、电能质量采集',
    task: {
      deviceId: 'meter-1',
      enabled: true,
      intervalMs: 1000,
      pointIds: ['meter_voltage_a', 'meter_current_a'],
      taskId: 'meter-fast-scan',
    },
    templateId: 'modbus-rtu-meter-basic',
    version: 'v1.2.0',
  },
  {
    algorithm: {
      algorithmId: 'pump-pressure-change',
      algorithmKind: 'ChangeReport',
      dsl: {
        inputs: [{ alias: 'pressure', pointId: 'pump_pressure' }],
        report: { mode: 'OnChange', sink: 'velamq-main' },
        steps: [{ type: 'changeFilter', source: 'pressure', threshold: 0.2 }],
        outputs: [{ name: 'reported', pointId: 'pump_pressure.reported' }],
        trigger: { type: 'onSample' },
      },
      version: '1.0.0',
    },
    connection: { endpoint: '/dev/ttyUSB1', protocolType: 'ModbusRtu' },
    dataConfig: {
      collection: { periodMs: 1000, retryCount: 2, timeoutMs: 800 },
      configId: 'pump_status_telemetry',
      deviceId: 'pump-1',
      enabled: true,
      name: '泵站状态上报',
      points: [
        {
          addressKind: 'holding_register',
          addressValue: '40011',
          jsonField: 'pressure',
          pointId: 'pump_pressure',
          semanticId: 'pump.pressure',
          unit: 'MPa',
          valueType: 'float32',
        },
        {
          addressKind: 'coil',
          addressValue: '00001',
          jsonField: 'running',
          pointId: 'pump_running',
          semanticId: 'pump.running',
          unit: '-',
          valueType: 'bool',
        },
      ],
      publish: {
        payload: { includeQuality: true, mode: 'object', timestampField: 'ts' },
        qos: 1,
        sinkId: 'velamq-main',
        topicTemplate: 'factory/{edge_id}/pump/{device_id}/status',
      },
    },
    description:
      '面向泵站或产线辅机，初始化压力和运行状态点位，并配置变化上报，减少 MQTT 侧无效消息量。',
    highlights: ['压力变化上报', '运行状态点位', '单设备多点组合 JSON', '低噪声上报'],
    mqtt: {
      batchSize: 80,
      broker: 'mqtts://velamq.local:8883',
      clientId: '{edge_id}-runtime',
      flushIntervalMs: 1000,
      qos: 1,
      sinkId: 'velamq-main',
      topicTemplate: 'factory/{edge_id}/{device_id}/telemetry',
    },
    name: '泵站状态模板',
    points: [
      {
        addressKind: 'holding_register',
        addressValue: '40011',
        deviceId: 'pump-1',
        intervalMs: 1000,
        pointId: 'pump_pressure',
        semanticId: 'pump.pressure',
        unit: 'MPa',
        valueType: 'float32',
      },
      {
        addressKind: 'coil',
        addressValue: '00001',
        deviceId: 'pump-1',
        intervalMs: 1000,
        pointId: 'pump_running',
        semanticId: 'pump.running',
        unit: '-',
        valueType: 'bool',
      },
    ],
    productType: 'pump-station',
    projectId: 'demo-plant',
    recommendedFor: '泵站、空压机、产线辅机',
    task: {
      deviceId: 'pump-1',
      enabled: true,
      intervalMs: 1000,
      pointIds: ['pump_pressure', 'pump_running'],
      taskId: 'pump-main-scan',
    },
    templateId: 'pump-collection-uplink',
    version: 'v1.4.3',
  },
  {
    algorithm: {
      algorithmId: 'energy-threshold-alert',
      algorithmKind: 'ThresholdRule',
      dsl: {
        inputs: [{ alias: 'power', pointId: 'energy_power' }],
        report: { mode: 'EventOnly', sink: 'velamq-main' },
        steps: [
          {
            event: {
              code: 'ENERGY_POWER_HIGH',
              message: '功率超过阈值',
              severity: 'Warning',
            },
            operator: 'Gt',
            source: 'power',
            threshold: 120,
            type: 'thresholdRule',
          },
        ],
        outputs: [{ name: 'alert', pointId: 'energy_power.alert' }],
        trigger: { type: 'onAnyInput' },
      },
      version: '1.0.0',
    },
    connection: { endpoint: '/dev/ttyUSB0', protocolType: 'ModbusRtu' },
    dataConfig: {
      collection: { periodMs: 5000, retryCount: 2, timeoutMs: 1000 },
      configId: 'energy_aggregate_report',
      deviceId: 'energy-meter-1',
      enabled: true,
      name: '能耗聚合上报',
      points: [
        {
          addressKind: 'holding_register',
          addressValue: '40101',
          jsonField: 'power',
          pointId: 'energy_power',
          semanticId: 'energy.power',
          unit: 'kW',
          valueType: 'float32',
        },
      ],
      publish: {
        payload: { includeQuality: true, mode: 'object', timestampField: 'ts' },
        qos: 1,
        sinkId: 'velamq-main',
        topicTemplate: 'factory/{edge_id}/energy/{device_id}/aggregate',
      },
    },
    description:
      '面向能耗看板，将功率点位按较低频率汇聚上报，并生成阈值告警事件，适合作为报表型数据流水线起点。',
    highlights: ['5 秒采集', '聚合主题', '阈值告警', '报表型数据'],
    mqtt: {
      batchSize: 60,
      broker: 'mqtts://velamq.local:8883',
      clientId: '{edge_id}-runtime',
      flushIntervalMs: 2000,
      qos: 1,
      sinkId: 'velamq-main',
      topicTemplate: 'factory/{edge_id}/{device_id}/telemetry',
    },
    name: '能耗聚合模板',
    points: [
      {
        addressKind: 'holding_register',
        addressValue: '40101',
        deviceId: 'energy-meter-1',
        intervalMs: 5000,
        pointId: 'energy_power',
        semanticId: 'energy.power',
        unit: 'kW',
        valueType: 'float32',
      },
    ],
    productType: 'energy',
    projectId: 'demo-plant',
    recommendedFor: '能耗计量、园区看板、周期报表',
    task: {
      deviceId: 'energy-meter-1',
      enabled: true,
      intervalMs: 5000,
      pointIds: ['energy_power'],
      taskId: 'energy-slow-scan',
    },
    templateId: 'energy-window-report',
    version: 'v1.1.0',
  },
];

const DEFAULT_EDGE_TEMPLATE_ID: EdgeTemplateId = 'pump-collection-uplink';

interface MaterializedProductRuntime {
  algorithms: Array<CreateAlgorithmRequest & { algorithmId: string }>;
  dataConfig: SaveDataConfigRequest;
}

export function materializeProductRuntime(
  template: EdgeTemplateDefinition,
  protocolConnectionId: string,
): MaterializedProductRuntime {
  if (!template.dataConfig.visualGraph?.nodes.length) {
    return {
      algorithms: [template.algorithm],
      dataConfig: buildLegacyTemplateDataConfigRequest(template, protocolConnectionId),
    };
  }

  const graph = template.dataConfig.visualGraph;
  const orderedNodes = topologicalProductNodes(graph);
  const runtimeAlgorithmIds = new Map<string, string>();
  for (const node of orderedNodes) {
    if (node.kind !== 'algorithm' || !isExecutableProductCompute(node.refId)) continue;
    runtimeAlgorithmIds.set(
      node.nodeId,
      `${template.dataConfig.configId}__${stableProductNodeId(node.nodeId)}`,
    );
  }

  const algorithms = orderedNodes.flatMap((node) => {
    const algorithmId = runtimeAlgorithmIds.get(node.nodeId);
    if (!algorithmId) return [];
    return [
      buildProductComputeAlgorithm(
        template,
        graph,
        node,
        algorithmId,
        runtimeAlgorithmIds,
      ),
    ];
  });
  const materializedGraph: DataConfigVisualGraph = {
    edges: graph.edges,
    nodes: graph.nodes.map((node) => ({
      ...node,
      refId: runtimeAlgorithmIds.get(node.nodeId) ?? node.refId,
    })),
  };

  return {
    algorithms,
    dataConfig: {
      ...template.dataConfig,
      algorithmIds: algorithms.map((algorithm) => algorithm.algorithmId),
      protocolConnectionId,
      visualGraph: materializedGraph,
    },
  };
}

function buildLegacyTemplateDataConfigRequest(
  template: EdgeTemplateDefinition,
  protocolConnectionId: string,
): SaveDataConfigRequest {
  const pointNodes = template.dataConfig.points.map((point, index) => ({
    kind: 'point' as const,
    label: point.pointId,
    nodeId: `point-${point.pointId}`,
    refId: point.pointId,
    x: 52,
    y: 56 + index * 86,
  }));

  return {
    ...template.dataConfig,
    algorithmIds: [template.algorithm.algorithmId ?? ''],
    protocolConnectionId,
    visualGraph: {
      edges: [
        ...pointNodes.map((node) => ({
          edgeId: `${node.nodeId}-to-algorithm`,
          from: node.nodeId,
          to: 'algorithm-node',
        })),
        { edgeId: 'algorithm-to-mqtt', from: 'algorithm-node', to: 'mqtt-output' },
      ],
      nodes: [
        ...pointNodes,
        {
          kind: 'algorithm',
          label: template.algorithm.algorithmId ?? 'algorithm',
          nodeId: 'algorithm-node',
          refId: template.algorithm.algorithmId ?? null,
          x: 310,
          y: 90,
        },
        {
          kind: 'mqtt',
          label: template.dataConfig.publish.topicTemplate,
          nodeId: 'mqtt-output',
          refId: template.dataConfig.publish.topicTemplate,
          x: 620,
          y: 90,
        },
      ],
    },
  };
}

function isExecutableProductCompute(kind?: string | null) {
  return kind !== 'merge_points';
}

function stableProductNodeId(nodeId: string) {
  return nodeId
    .replace(/^algorithm-/, '')
    .replace(/[^a-zA-Z0-9_-]+/g, '-')
    .replace(/^-+|-+$/g, '') || 'compute';
}

function topologicalProductNodes(graph: DataConfigVisualGraph) {
  const indegree = new Map(graph.nodes.map((node) => [node.nodeId, 0]));
  for (const edge of graph.edges) {
    indegree.set(edge.to, (indegree.get(edge.to) ?? 0) + 1);
  }
  const queue = graph.nodes.filter((node) => (indegree.get(node.nodeId) ?? 0) === 0);
  const ordered: DataConfigVisualGraph['nodes'] = [];
  while (queue.length > 0) {
    const node = queue.shift();
    if (!node) break;
    ordered.push(node);
    for (const edge of graph.edges.filter((item) => item.from === node.nodeId)) {
      const next = (indegree.get(edge.to) ?? 0) - 1;
      indegree.set(edge.to, next);
      if (next === 0) {
        const target = graph.nodes.find((item) => item.nodeId === edge.to);
        if (target) queue.push(target);
      }
    }
  }
  for (const node of graph.nodes) {
    if (!ordered.some((item) => item.nodeId === node.nodeId)) ordered.push(node);
  }
  return ordered;
}

function buildProductComputeAlgorithm(
  template: EdgeTemplateDefinition,
  graph: DataConfigVisualGraph,
  node: DataConfigVisualGraph['nodes'][number],
  algorithmId: string,
  runtimeAlgorithmIds: Map<string, string>,
): CreateAlgorithmRequest & { algorithmId: string } {
  const pointIds = resolveProductComputeInputs(graph, node.nodeId, runtimeAlgorithmIds);
  const effectivePointIds = pointIds.length ? pointIds : [template.dataConfig.points[0]?.pointId ?? 'input'];
  const inputs = effectivePointIds.map((pointId, index) => ({ alias: `p${index}`, pointId }));
  const kind = node.refId ?? 'expression';
  const primaryAlias = inputs[0].alias;
  const sink = template.dataConfig.publish.sinkId;
  const outputBase = `${algorithmId}.output`;
  const params = node.params ?? {};

  if (kind === 'window_aggregate' || kind === 'moving_average' || kind === 'statistics') {
    const supportedFunctions = ['avg', 'min', 'max', 'sum', 'count', 'first', 'last'] as const;
    const requestedFunctions = Array.isArray(params.metrics)
      ? params.metrics.map(String)
      : kind === 'moving_average'
        ? ['avg']
        : kind === 'statistics'
          ? [...supportedFunctions]
          : ['avg', 'min', 'max', 'sum', 'count'];
    const functions = supportedFunctions.filter((name) => requestedFunctions.includes(name));
    return {
      algorithmId,
      algorithmKind: kind === 'statistics' ? 'Statistics' : 'WindowAggregate',
      dsl: {
        inputs,
        outputs: functions.map((name) => ({ name, pointId: `${outputBase}.${name}` })),
        report: { mode: 'WindowResult', sink },
        steps: [
          {
            functions: functions.map((name) => ({ function: name, output: name })),
            source: primaryAlias,
            type: 'windowAggregate',
          },
        ],
        trigger: {
          everyMs: Math.max(productNodeParamNumber(node, 'windowMs', 5000), 1),
          type: 'window',
        },
      },
      version: template.version,
    };
  }

  if (kind === 'change_report' || kind === 'deadband_filter') {
    return {
      algorithmId,
      algorithmKind: kind === 'deadband_filter' ? 'Deadband' : 'ChangeReport',
      dsl: {
        inputs,
        outputs: [{ name: 'value', pointId: outputBase }],
        report: { mode: 'OnChange', sink },
        steps: [
          {
            source: primaryAlias,
            threshold: Math.max(productNodeParamNumber(node, 'threshold', kind === 'deadband_filter' ? 0.1 : 0), 0),
            type: 'changeFilter',
          },
        ],
        trigger: { type: 'onSample' },
      },
      version: template.version,
    };
  }

  if (kind === 'debounce') {
    return {
      algorithmId,
      algorithmKind: 'Debounce',
      dsl: {
        inputs,
        outputs: [{ name: 'value', pointId: outputBase }],
        report: { mode: 'OnOutput', sink },
        steps: [{ source: primaryAlias, stableMs: Math.max(productNodeParamNumber(node, 'stableMs', 1000), 1), type: 'debounce' }],
        trigger: { type: 'onSample' },
      },
      version: template.version,
    };
  }

  if (kind === 'duration_condition') {
    return {
      algorithmId,
      algorithmKind: 'DurationRule',
      dsl: {
        inputs,
        outputs: [{ name: 'value', pointId: outputBase }],
        report: { mode: 'OnOutput', sink },
        steps: [{
          durationMs: Math.max(productNodeParamNumber(node, 'durationMs', 5000), 1),
          operator: productNodeCompareOperator(node),
          output: 'value',
          source: primaryAlias,
          threshold: productNodeParamNumber(node, 'threshold', 0),
          type: 'durationCondition',
        }],
        trigger: { type: 'onSample' },
      },
      version: template.version,
    };
  }

  if (kind === 'scale_offset') {
    return {
      algorithmId,
      algorithmKind: 'ExpressionAggregate',
      dsl: {
        inputs,
        outputs: [{ name: 'value', pointId: outputBase }],
        report: { mode: 'OnOutput', sink },
        steps: [{
          factor: productNodeParamNumber(node, 'factor', 1),
          offset: productNodeParamNumber(node, 'offset', 0),
          output: 'value',
          source: primaryAlias,
          type: 'scale',
        }],
        trigger: { type: 'onSample' },
      },
      version: template.version,
    };
  }

  if (kind === 'clamp') {
    const min = productNodeParamNumber(node, 'min', 0);
    const max = productNodeParamNumber(node, 'max', 100);
    return {
      algorithmId,
      algorithmKind: 'ExpressionAggregate',
      dsl: {
        inputs,
        outputs: [{ name: 'value', pointId: outputBase }],
        report: { mode: 'OnOutput', sink },
        steps: [{ max: Math.max(min, max), min: Math.min(min, max), output: 'value', source: primaryAlias, type: 'clamp' }],
        trigger: { type: 'onSample' },
      },
      version: template.version,
    };
  }

  if (kind === 'rate_of_change') {
    return {
      algorithmId,
      algorithmKind: 'ExpressionAggregate',
      dsl: {
        inputs,
        outputs: [{ name: 'value', pointId: outputBase }],
        report: { mode: 'OnOutput', sink },
        steps: [{ output: 'value', perMs: Math.max(productNodeParamNumber(node, 'perMs', 1000), 1), source: primaryAlias, type: 'rateOfChange' }],
        trigger: { type: 'onSample' },
      },
      version: template.version,
    };
  }

  if (kind === 'condition_route') {
    return {
      algorithmId,
      algorithmKind: 'ThresholdRule',
      dsl: {
        inputs,
        outputs: [
          { name: 'matched', pointId: `${outputBase}.matched` },
          { name: 'unmatched', pointId: `${outputBase}.unmatched` },
        ],
        report: { mode: 'OnOutput', sink },
        steps: [{
          matchedOutput: 'matched',
          operator: productNodeCompareOperator(node),
          source: primaryAlias,
          threshold: productNodeParamNumber(node, 'threshold', 0),
          type: 'conditionalRoute',
          unmatchedOutput: 'unmatched',
        }],
        trigger: { type: 'onSample' },
      },
      version: template.version,
    };
  }

  if (kind === 'alarm_event') {
    return {
      algorithmId,
      algorithmKind: 'ThresholdRule',
      dsl: {
        inputs,
        outputs: [{ name: 'event', pointId: outputBase }],
        report: { mode: 'EventOnly', sink },
        steps: [
          {
            event: {
              code: `${stableProductNodeId(node.nodeId).toUpperCase()}_ALARM`,
              message: `${node.label}触发`,
              severity: 'Warning',
            },
            operator: productNodeCompareOperator(node),
            source: primaryAlias,
            threshold: productNodeParamNumber(node, 'threshold', 0),
            type: 'thresholdRule',
          },
        ],
        trigger: { type: 'onAnyInput' },
      },
      version: template.version,
    };
  }

  return {
    algorithmId,
    algorithmKind: 'ExpressionAggregate',
    dsl: {
      inputs,
      outputs: [{ name: 'value', pointId: outputBase }],
      report: { mode: 'OnOutput', sink },
      steps: [
        {
          expr: String(params.expression ?? inputs.map((input) => input.alias).join(' + ')),
          output: 'value',
          type: 'expression',
        },
      ],
      trigger: { type: 'onAnyInput' },
    },
    version: template.version,
  };
}

function resolveProductComputeInputs(
  graph: DataConfigVisualGraph,
  nodeId: string,
  runtimeAlgorithmIds: Map<string, string>,
  visited = new Set<string>(),
): string[] {
  if (visited.has(nodeId)) return [];
  const nextVisited = new Set(visited);
  nextVisited.add(nodeId);
  const pointIds = graph.edges
    .filter((edge) => edge.to === nodeId)
    .flatMap((edge) => {
      const source = graph.nodes.find((node) => node.nodeId === edge.from);
      if (!source) return [];
      if (source.kind === 'point') return source.refId ? [source.refId] : [];
      const upstreamAlgorithmId = runtimeAlgorithmIds.get(source.nodeId);
      if (upstreamAlgorithmId) {
        return [productComputeOutputId(source.refId, upstreamAlgorithmId, edge.fromPort)];
      }
      return resolveProductComputeInputs(graph, source.nodeId, runtimeAlgorithmIds, nextVisited);
    });
  return Array.from(new Set(pointIds));
}

function productComputeOutputId(
  kind: string | null | undefined,
  algorithmId: string,
  outputPort?: string | null,
) {
  if (kind === 'condition_route' && (outputPort === 'matched' || outputPort === 'unmatched')) {
    return `${algorithmId}.output.${outputPort}`;
  }
  return kind === 'window_aggregate' || kind === 'moving_average' || kind === 'statistics'
    ? `${algorithmId}.output.avg`
    : `${algorithmId}.output`;
}

function productNodeParamNumber(
  node: DataConfigVisualGraph['nodes'][number],
  key: string,
  fallback: number,
) {
  const value = Number(node.params?.[key] ?? fallback);
  return Number.isFinite(value) ? value : fallback;
}

function productNodeCompareOperator(node: DataConfigVisualGraph['nodes'][number]) {
  const operator = String(node.params?.operator ?? 'Gt');
  return ['Gt', 'Gte', 'Lt', 'Lte', 'Eq', 'Ne'].includes(operator)
    ? (operator as 'Gt' | 'Gte' | 'Lt' | 'Lte' | 'Eq' | 'Ne')
    : 'Gt';
}

function buildEdgeProductOptions(
  products: EdgeTemplateDefinition[],
  projects: ProjectDefinition[],
): EdgeProductOption[] {
  return products.map((product) => ({
    productId: product.templateId,
    productName: product.name,
    projectId: product.projectId,
    projectName: projectName(projects, product.projectId),
    version: product.version,
  }));
}

function buildEdgeConfigSummaries(
  edges: EdgeNodeResponse[] = [],
  protocolConnections: ProtocolConnectionResponse[] = [],
  pointMappings: PointMappingResponse[] = [],
  collectionTasks: CollectionTaskResponse[] = [],
  dataConfigs: DataConfigResponse[] = [],
  mqttUplink?: MqttUplinkResponse,
  releaseList?: ReleaseListResponse,
  products: EdgeTemplateDefinition[] = [],
  projects: ProjectDefinition[] = [],
  bindings: EdgeProductBinding[] = [],
): EdgeConfigSummary[] {
  return edges.map((edge) => {
    const releaseResult = releaseList?.applyResults.find(
      (result) => result.edgeId === edge.edgeId,
    );
    const binding = bindings.find((item) => item.edgeId === edge.edgeId);
    const product = products.find((item) => item.templateId === binding?.productId);
    const project = projects.find((item) => item.projectId === binding?.projectId);
    return {
      collectionTaskCount: collectionTasks.filter((task) => task.edgeId === edge.edgeId)
        .length,
      dataConfigCount: dataConfigs.filter((config) => config.edgeId === edge.edgeId)
        .length,
      edgeId: edge.edgeId,
      productName: product?.name ?? '未绑定产品',
      productVersion: binding?.desiredVersion ?? product?.version ?? '-',
      projectName: project?.projectName ?? '未分配项目',
      mqttSinkId: mqttUplink?.sinkId ?? '未配置',
      pointCount: pointMappings.filter((point) => point.edgeId === edge.edgeId).length,
      protocolCount: protocolConnections.filter(
        (connection) => connection.edgeId === edge.edgeId,
      ).length,
      releaseStatus: formatReleaseBindingStatus(releaseResult),
    };
  });
}

function upsertEdgeProductBinding(
  bindings: EdgeProductBinding[],
  nextBinding: EdgeProductBinding,
) {
  const exists = bindings.some((binding) => binding.edgeId === nextBinding.edgeId);
  if (!exists) {
    return [nextBinding, ...bindings];
  }
  return bindings.map((binding) =>
    binding.edgeId === nextBinding.edgeId ? nextBinding : binding,
  );
}

function edgeProductBindingFromNode(edge: EdgeNodeResponse): EdgeProductBinding {
  return {
    bindingStatus: edge.productId ? '已绑定' : '未绑定',
    desiredVersion: edge.desiredProductVersion ?? '-',
    edgeId: edge.edgeId,
    loadedAt: edge.heartbeat,
    productId: (edge.productId ?? '') as EdgeTemplateId,
    projectId: edge.projectId ?? '',
    reportedVersion: edge.reportedProductVersion ?? '-',
  };
}

function renderPage(
  activePage: PageKey,
  summary: SummaryResponse,
  loadState: 'loading' | 'ready' | 'error',
  edgeConfigurationMode: EdgeConfigurationMode,
  focusedRuntimeEdgeId: string | undefined,
  edgeConfigInitialTab: EdgeConfigTabKey,
  edgeTemplates: EdgeTemplateDefinition[],
  projects: ProjectDefinition[],
  edgeProductBindings: EdgeProductBinding[],
  edgeAccessTokens: EdgeAccessTokens,
  onCreateProject: () => Promise<ProjectDefinition>,
  onSaveProject: (
    projectId: string,
    nextProject: ProjectDefinition,
  ) => Promise<ProjectDefinition>,
  onDeleteProject: (projectId: string) => Promise<void>,
  pointSets: PointSetResponse[],
  onCreatePointSet: (request: SavePointSetRequest) => Promise<PointSetResponse>,
  onSavePointSet: (
    pointSetId: string,
    request: SavePointSetRequest,
  ) => Promise<PointSetResponse>,
  onDeletePointSet: (pointSetId: string) => Promise<void>,
  onCreateEdgeTemplate: () => Promise<EdgeTemplateDefinition>,
  onSaveEdgeTemplate: (
    templateId: EdgeTemplateId,
    nextTemplate: EdgeTemplateDefinition,
  ) => Promise<EdgeTemplateDefinition>,
  onDeleteEdgeTemplate: (templateId: EdgeTemplateId) => Promise<void>,
  productVersions: Record<string, ProductVersionResponse[]>,
  onPublishProductVersion: (
    productId: string,
    version: string,
  ) => Promise<ProductVersionResponse>,
  onRollbackProductVersion: (
    productId: string,
    version: string,
  ) => Promise<ProductVersionResponse>,
  onAgentSafetyCheck: () => Promise<AgentActionResponse>,
  onAssessAlgorithmRisk: (edgeId: string) => Promise<ManagementActionResponse>,
  onPrepareConfigSection: (edgeId: string) => Promise<void>,
  onCreateManagedEdge: (request: CreateManagedEdgeRequest) => Promise<EdgeNodeResponse>,
  onGenerateEdgeAccessToken: (edgeId: string) => Promise<string>,
  onDeleteEdge: (edgeId: string) => Promise<void>,
  onCreateAlgorithm: (
    edgeId: string,
    request: CreateAlgorithmRequest,
  ) => Promise<AlgorithmResponse>,
  onSaveAlgorithm: (
    edgeId: string,
    algorithmId: string,
    request: SaveAlgorithmRequest,
  ) => Promise<void>,
  onDeleteAlgorithm: (edgeId: string, algorithmId: string) => Promise<void>,
  onCreateCollectionTask: (
    edgeId: string,
    request: CreateCollectionTaskRequest,
  ) => Promise<CollectionTaskResponse>,
  onDeleteCollectionTask: (edgeId: string, taskId: string) => Promise<void>,
  onCreateDeviceModel: (request: CreateDeviceModelRequest) => Promise<DeviceModelResponse>,
  onSaveDeviceModel: (
    deviceType: string,
    request: SaveDeviceModelRequest,
  ) => Promise<DeviceModelResponse>,
  onDeleteDeviceModel: (deviceType: string) => Promise<void>,
  onCreatePoint: (
    edgeId?: string,
    request?: CreatePointMappingRequest,
  ) => Promise<PointMappingResponse>,
  onDeletePoint: (edgeId: string, pointId: string) => Promise<void>,
  onGenerateAgentSuggestions: () => Promise<AgentActionResponse>,
  onAgentChat: (request: AgentChatRequest) => Promise<AgentChatResponse>,
  onAgentProviderStatus: () => Promise<AgentProviderStatusResponse>,
  onListAgentConversations: (
    projectId?: string,
  ) => Promise<AgentConversationResponse[]>,
  onDeleteAgentConversation: (conversationId: string) => Promise<void>,
  onListAgentKnowledge: (
    projectId?: string,
  ) => Promise<AgentKnowledgeDocumentResponse[]>,
  onSaveAgentKnowledge: (
    documentId: string | null,
    request: SaveAgentKnowledgeDocumentRequest,
  ) => Promise<AgentKnowledgeDocumentResponse>,
  onDeleteAgentKnowledge: (documentId: string) => Promise<void>,
  onCreateAgentProposal: (
    request: CreateAgentProposalRequest,
  ) => Promise<AgentProposalResponse>,
  onListAgentProposals: () => Promise<AgentProposalResponse[]>,
  onReviewAgentProposal: (
    proposalId: string,
    decision: 'approve' | 'reject',
    request: ReviewAgentProposalRequest,
  ) => Promise<AgentProposalResponse>,
  onRunDiscovery: (
    edgeId: string,
    request: RunDiscoveryRequest,
  ) => Promise<DiscoveryReportResponse>,
  onGenerateSchedule: (edgeId: string) => Promise<ManagementActionResponse>,
  onImportPoints: (edgeId: string) => Promise<ManagementActionResponse>,
  onApplyEdgeTemplate: (
    edgeId: string,
    templateId: EdgeTemplateId,
  ) => Promise<ManagementActionResponse>,
  onMonitorEdge: (edgeId: string) => void,
  onReleaseDiff: (edgeId: string) => Promise<ManagementActionResponse>,
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
  onSaveDataConfig: (
    edgeId: string,
    configId: string | null,
    request: SaveDataConfigRequest,
  ) => Promise<void>,
  onDeleteDataConfig: (edgeId: string, configId: string) => Promise<void>,
  selectedDataConfigEdgeId: string,
  onSaveProtocolConnection: (
    edgeId: string,
    connectionId: string,
    request: SaveProtocolConnectionRequest,
  ) => Promise<void>,
  onCreateProtocolConnection: (
    edgeId: string,
    request: CreateProtocolConnectionRequest,
  ) => Promise<ProtocolConnectionResponse>,
  onDeleteProtocolConnection: (
    edgeId: string,
    connectionId: string,
  ) => Promise<void>,
  selectedProtocolEdgeId: string,
  onSaveMqttUplink: (
    edgeId: string,
    request: MqttUplinkResponse,
  ) => Promise<MqttUplinkResponse>,
  onPublish: (edgeId: string) => Promise<void>,
  onValidateConfig: (edgeId?: string) => Promise<ManagementActionResponse>,
  edgeNodes?: EdgeNodeResponse[],
  deviceModels?: DeviceModelResponse[],
  protocolConnections?: ProtocolConnectionResponse[],
  mqttUplink?: MqttUplinkResponse,
  discoverySuggestions?: PointMappingSuggestionResponse[],
  pointMappings?: PointMappingResponse[],
  collectionTasks?: CollectionTaskResponse[],
  dataConfigs?: DataConfigResponse[],
  algorithms?: AlgorithmResponse[],
  releaseList?: ReleaseListResponse,
  runtimeStatus?: RuntimeStatusResponse,
  auditRecords?: AuditRecordResponse[],
  principal?: AuthStatusResponse,
) {
  switch (activePage) {
    case 'dashboard':
      return (
        <DashboardPage
          auditRecords={auditRecords}
          edgeNodes={edgeNodes}
          loadState={loadState}
          runtimeStatus={runtimeStatus}
          summary={summary}
        />
      );
    case 'edges':
      return (
        <EdgeNodesPage
          accessTokens={edgeAccessTokens}
          configSummaries={buildEdgeConfigSummaries(
            edgeNodes,
            protocolConnections,
            pointMappings,
            collectionTasks,
            dataConfigs,
            mqttUplink,
            releaseList,
            edgeTemplates,
            projects,
            edgeProductBindings,
          )}
          edges={edgeNodes}
          mqttUplink={mqttUplink}
          onCreateEdge={onCreateManagedEdge}
          onDeleteEdge={onDeleteEdge}
          onGenerateAccessToken={onGenerateEdgeAccessToken}
          onSaveMqttUplink={onSaveMqttUplink}
          products={buildEdgeProductOptions(
            edgeTemplates.filter((template) =>
              productVersions[template.templateId]?.some(
                (version) => version.status === 'published',
              ),
            ),
            projects,
          )}
        />
      );
    case 'edgeConfig':
      return (
        <EdgeConfigWorkspace
          algorithms={algorithms}
          collectionTasks={collectionTasks}
          dataConfigs={dataConfigs}
          discoverySuggestions={discoverySuggestions}
          edgeId={selectedProtocolEdgeId}
          edges={edgeNodes}
          initialTab={edgeConfigInitialTab}
          mqttUplink={mqttUplink}
          edgeTemplates={edgeTemplates}
          projects={projects}
          productBinding={edgeProductBindings.find(
            (binding) => binding.edgeId === selectedProtocolEdgeId,
          )}
          onAssessAlgorithmRisk={onAssessAlgorithmRisk}
          onCreateAlgorithm={onCreateAlgorithm}
          onCreateCollectionTask={onCreateCollectionTask}
          onCreatePoint={onCreatePoint}
          onCreateProtocolConnection={onCreateProtocolConnection}
          onDeleteAlgorithm={onDeleteAlgorithm}
          onDeleteCollectionTask={onDeleteCollectionTask}
          onDeleteDataConfig={onDeleteDataConfig}
          onDeletePoint={onDeletePoint}
          onDeleteProtocolConnection={onDeleteProtocolConnection}
          onGenerateSchedule={onGenerateSchedule}
          onApplyEdgeTemplate={onApplyEdgeTemplate}
          onImportPoints={onImportPoints}
          onPublish={onPublish}
          onReleaseDiff={onReleaseDiff}
          onRunDiscovery={onRunDiscovery}
          onSaveAlgorithm={onSaveAlgorithm}
          onSaveCollectionTask={onSaveCollectionTask}
          onSaveDataConfig={onSaveDataConfig}
          onSaveMqttUplink={onSaveMqttUplink}
          onSavePoint={onSavePoint}
          onSaveProtocolConnection={onSaveProtocolConnection}
          onValidateConfig={onValidateConfig}
          pointMappings={pointMappings}
          protocolConnections={protocolConnections}
          releaseList={releaseList}
        />
      );
    case 'projects':
      return (
        <ProjectManagementPage
          bindings={edgeProductBindings}
          edgeNodes={edgeNodes}
          onCreateProject={onCreateProject}
          onDeleteProject={onDeleteProject}
          onSaveProject={onSaveProject}
          products={edgeTemplates}
          projects={projects}
        />
      );
    case 'products':
      return (
        <ProductManagementPage
          dataConfigs={dataConfigs}
          onCreateTemplate={onCreateEdgeTemplate}
          onDeleteTemplate={onDeleteEdgeTemplate}
          onPublishVersion={onPublishProductVersion}
          onRollbackVersion={onRollbackProductVersion}
          onSaveTemplate={onSaveEdgeTemplate}
          pointSets={pointSets}
          projects={projects}
          templates={edgeTemplates}
          versions={productVersions}
        />
      );
    case 'points':
      return (
        <PointSetsPage
          onCreate={onCreatePointSet}
          onDelete={onDeletePointSet}
          onSave={onSavePointSet}
          pointSets={pointSets}
          projects={projects.map((project) => ({
            name: project.projectName,
            projectId: project.projectId,
          }))}
        />
      );
    case 'deviceModels':
      return (
        <DeviceModelsPage
          deviceModels={deviceModels}
          onCreateDeviceModel={onCreateDeviceModel}
          onDeleteDeviceModel={onDeleteDeviceModel}
          onSaveDeviceModel={onSaveDeviceModel}
        />
      );
    case 'protocolConnections':
      return (
        <ProtocolConnectionsPage
          connections={protocolConnections}
          edges={edgeNodes}
          mode={edgeConfigurationMode}
          onCreateConnection={onCreateProtocolConnection}
          onDeleteConnection={onDeleteProtocolConnection}
          onSaveConnection={onSaveProtocolConnection}
          onValidateConnection={onValidateConfig}
          selectedEdgeId={selectedProtocolEdgeId}
        />
      );
    case 'dataConfigs':
      return (
        <DataConfigsPage
          algorithms={algorithms}
          configs={dataConfigs}
          edges={edgeNodes}
          mqttUplink={mqttUplink}
          onDeleteConfig={onDeleteDataConfig}
          onSaveConfig={onSaveDataConfig}
          pointMappings={pointMappings}
          protocolConnections={protocolConnections}
          selectedEdgeId={selectedDataConfigEdgeId}
        />
      );
    case 'discovery':
      return (
        <DiscoveryPage
          onRunDiscovery={onRunDiscovery}
          selectedEdgeId={defaultConfigEdgeId}
          suggestions={discoverySuggestions}
        />
      );
    case 'releases':
      return (
        <ReleasesPage
          edges={edgeNodes}
          onPublish={onPublish}
          onShowDiff={onReleaseDiff}
          onValidateRelease={onValidateConfig}
          releaseList={releaseList}
        />
      );
    case 'runtimeStatus':
      return (
        <RuntimeStatusPage
          focusedEdgeId={focusedRuntimeEdgeId}
          runtimeStatus={runtimeStatus}
        />
      );
    case 'auditLog':
      return <AuditLogPage auditRecords={auditRecords} />;
    case 'agentAssistant':
      return (
        <AgentAssistantPage
          canReviewProposals={principal?.role === 'admin'}
          onChat={onAgentChat}
          onDeleteKnowledge={onDeleteAgentKnowledge}
          onCreateProposal={onCreateAgentProposal}
          onGenerateSuggestions={onGenerateAgentSuggestions}
          onGetProviderStatus={onAgentProviderStatus}
          onListConversations={onListAgentConversations}
          onDeleteConversation={onDeleteAgentConversation}
          onListKnowledge={onListAgentKnowledge}
          onListProposals={onListAgentProposals}
          onReviewProposal={onReviewAgentProposal}
          onRunSafetyCheck={onAgentSafetyCheck}
          onSaveKnowledge={onSaveAgentKnowledge}
          projectOptions={projects.map((project) => ({
            projectId: project.projectId,
            projectName: project.projectName,
          }))}
        />
      );
  }
}

type EdgeConfigTab =
  | 'versions'
  | 'protocol'
  | 'points'
  | 'collection'
  | 'algorithms'
  | 'reports'
  | 'mqtt'
  | 'discovery'
  | 'release';

function EdgeConfigVersionPanel({
  edgeId,
  onReleaseDiff,
  onApplyTemplate,
  onPublish,
  onValidateConfig,
  productBinding,
  projects,
  releaseList,
  templates,
}: {
  edgeId: string;
  onReleaseDiff: (edgeId: string) => Promise<ManagementActionResponse>;
  onApplyTemplate: (
    edgeId: string,
    templateId: EdgeTemplateId,
  ) => Promise<ManagementActionResponse>;
  onPublish: (edgeId: string) => Promise<void>;
  onValidateConfig: (edgeId?: string) => Promise<ManagementActionResponse>;
  productBinding?: EdgeProductBinding;
  projects: ProjectDefinition[];
  releaseList?: ReleaseListResponse;
  templates: EdgeTemplateDefinition[];
}) {
  const initialProductId = productBinding?.productId ?? DEFAULT_EDGE_TEMPLATE_ID;
  const initialProjectId =
    productBinding?.projectId ??
    templates.find((template) => template.templateId === initialProductId)?.projectId ??
    projects[0]?.projectId ??
    'demo-plant';
  const [selectedProjectId, setSelectedProjectId] = useState(initialProjectId);
  const [selectedTemplateId, setSelectedTemplateId] =
    useState<EdgeTemplateId>(initialProductId);
  const [actionState, setActionState] = useState<'idle' | 'running' | 'done' | 'error'>('idle');
  const [actionResult, setActionResult] = useState<ManagementActionResponse>();
  const projectProducts = templates.filter(
    (template) => template.projectId === selectedProjectId,
  );
  const selectedTemplate =
    templates.find((template) => template.templateId === selectedTemplateId) ??
    projectProducts[0] ??
    templates[0];
  const selectedProject =
    projects.find((project) => project.projectId === selectedTemplate.projectId) ??
    projects.find((project) => project.projectId === selectedProjectId);
  const releaseResult = releaseList?.applyResults.find((result) => result.edgeId === edgeId);

  const applyTemplate = async () => {
    setActionState('running');
    setActionResult(undefined);
    try {
      const result = await onApplyTemplate(edgeId, selectedTemplate.templateId);
      setActionResult(result);
      setActionState('done');
    } catch (error) {
      setActionResult({
        action: 'apply_edge_template',
        details: [displayError(error, '请检查产品内协议、点位或上报配置是否与当前边端冲突')],
        message: '产品配置加载失败',
        status: '失败',
      });
      setActionState('error');
    }
  };
  const runAction = async (
    action: () => Promise<ManagementActionResponse> | Promise<void>,
    fallbackMessage: string,
  ) => {
    setActionState('running');
    setActionResult(undefined);
    try {
      const result = await action();
      setActionResult(
        result ?? {
          action: 'publish_release',
          details: ['配置包已生成，等待 runtime 拉取并回报应用结果'],
          message: fallbackMessage,
          status: '已提交',
        },
      );
      setActionState('done');
    } catch (error) {
      setActionResult({
        action: 'edge_config_version',
        details: [displayError(error)],
        message: '操作失败',
        status: '失败',
      });
      setActionState('error');
    }
  };

  return (
    <div className="edge-version-shell">
      <section className="edge-version-hero">
        <div>
          <span>边端产品绑定</span>
          <h3>关联产品、加载默认配置、校验并下发</h3>
          <p>选择项目和产品，生成当前边端的待发布配置。</p>
        </div>
        <div className="edge-version-actions">
          <button
            className="secondary-button"
            disabled={actionState === 'running'}
            onClick={() => void runAction(() => onValidateConfig(edgeId), '配置校验已完成')}
            type="button"
          >
            校验配置
          </button>
          <button
            className="secondary-button"
            disabled={actionState === 'running'}
            onClick={() => void runAction(() => onReleaseDiff(edgeId), '配置差异已生成')}
            type="button"
          >
            查看差异
          </button>
          <button
            className="primary-button"
            disabled={actionState === 'running'}
            onClick={() => void runAction(() => onPublish(edgeId), '已创建发布，等待 runtime 回报')}
            type="button"
          >
            发布下发
          </button>
        </div>
      </section>

      <div className="edge-version-grid">
        <section className="edge-version-panel">
          <div className="template-preview-header">
            <div>
              <span>当前边端</span>
              <strong>{edgeId}</strong>
            </div>
            <div>
              <span>关联项目</span>
              <strong>{selectedProject?.projectName ?? productBinding?.projectId ?? '未绑定'}</strong>
            </div>
            <div>
              <span>产品版本</span>
              <strong>{productBinding?.desiredVersion ?? selectedTemplate.version}</strong>
            </div>
          </div>

          <div className="edge-version-timeline">
            <PreviewStep title="1. 绑定产品" value={productBinding?.bindingStatus ?? '待绑定'} />
            <PreviewStep title="2. 加载配置" value="产品配置生成实例草稿" />
            <PreviewStep title="3. 校验差异" value={releaseList?.validationStatus ?? '待校验'} />
            <PreviewStep title="4. 发布下发" value={releaseResult?.result ?? '待发布'} />
          </div>

          <div className="edge-version-release">
            <h4>版本状态</h4>
            <dl>
              <div>
                <dt>产品</dt>
                <dd>{selectedTemplate.name}</dd>
              </div>
              <div>
                <dt>目标版本</dt>
                <dd>{productBinding?.desiredVersion ?? selectedTemplate.version}</dd>
              </div>
              <div>
                <dt>草稿版本</dt>
                <dd>{releaseList?.draftVersion ?? '未生成'}</dd>
              </div>
              <div>
                <dt>Runtime 回报</dt>
                <dd>{releaseResult?.reportedVersion ?? productBinding?.reportedVersion ?? '-'}</dd>
              </div>
            </dl>
          </div>
        </section>

        <section className="edge-version-panel">
          <div className="default-template-editor">
            <label className="editor-control">
              <span>所属项目</span>
              <select
                aria-label="选择项目"
                value={selectedProjectId}
                onChange={(event) => {
                  const nextProjectId = event.target.value;
                  const nextProduct = templates.find(
                    (template) => template.projectId === nextProjectId,
                  );
                  setSelectedProjectId(nextProjectId);
                  if (nextProduct) setSelectedTemplateId(nextProduct.templateId);
                  setActionResult(undefined);
                  setActionState('idle');
                }}
              >
                {projects.map((project) => (
                  <option key={project.projectId} value={project.projectId}>
                    {project.projectName}
                  </option>
                ))}
              </select>
            </label>
            <label className="editor-control">
              <span>关联产品</span>
              <select
                aria-label="选择关联产品"
                value={selectedTemplate.templateId}
                onChange={(event) => {
                  setSelectedTemplateId(event.target.value);
                  setActionResult(undefined);
                  setActionState('idle');
                }}
              >
                {(projectProducts.length > 0 ? projectProducts : templates).map((template) => (
                  <option key={template.templateId} value={template.templateId}>
                    {template.name} · {template.version}
                  </option>
                ))}
              </select>
            </label>
            <button
              className="primary-button"
              disabled={actionState === 'running'}
              onClick={applyTemplate}
              type="button"
            >
              {actionState === 'running' ? '加载中...' : '加载产品配置'}
            </button>
          </div>

          <div className="template-preview-table">
            <h4>{selectedTemplate.name}</h4>
            <p>{selectedTemplate.description}</p>
            <dl>
              <div>
                <dt>项目/产品类型</dt>
                <dd>{selectedProject?.projectName ?? selectedTemplate.projectId} · {selectedTemplate.productType}</dd>
              </div>
              <div>
                <dt>协议</dt>
                <dd>{selectedTemplate.connection.protocolType} · {selectedTemplate.connection.endpoint}</dd>
              </div>
              <div>
                <dt>点位/周期</dt>
                <dd>{selectedTemplate.points.length} 个点位 · {selectedTemplate.task.intervalMs}ms</dd>
              </div>
              <div>
                <dt>算法/上报</dt>
                <dd>{selectedTemplate.algorithm.algorithmKind} · {selectedTemplate.dataConfig.publish.topicTemplate}</dd>
              </div>
            </dl>
          </div>

          {actionResult ? (
            <div className={actionState === 'error' ? 'template-result error' : 'template-result'}>
              <strong>{actionResult.message}</strong>
              <ul>
                {actionResult.details.map((detail) => (
                  <li key={detail}>{detail}</li>
                ))}
              </ul>
            </div>
          ) : null}
        </section>
      </div>
    </div>
  );
}

function PreviewStep({ title, value }: { title: string; value: string }) {
  return (
    <div className="template-preview-step">
      <span>{title}</span>
      <strong>{value}</strong>
    </div>
  );
}

function projectName(projects: ProjectDefinition[], projectId: string) {
  return projects.find((project) => project.projectId === projectId)?.projectName ?? projectId;
}

function ProjectManagementPage({
  bindings,
  edgeNodes = [],
  onCreateProject,
  onDeleteProject,
  onSaveProject,
  products,
  projects,
}: {
  bindings: EdgeProductBinding[];
  edgeNodes?: EdgeNodeResponse[];
  onCreateProject: () => Promise<ProjectDefinition>;
  onDeleteProject: (projectId: string) => Promise<void>;
  onSaveProject: (
    projectId: string,
    nextProject: ProjectDefinition,
  ) => Promise<ProjectDefinition>;
  products: EdgeTemplateDefinition[];
  projects: ProjectDefinition[];
}) {
  const [selectedProjectId, setSelectedProjectId] = useState<string>();
  const [projectDraft, setProjectDraft] = useState<ProjectDefinition>();
  const [saveState, setSaveState] = useState<
    'idle' | 'saving' | 'saved' | 'deleted' | 'error'
  >('idle');
  const [actionError, setActionError] = useState('');
  const selectedProject = projects.find((project) => project.projectId === selectedProjectId);
  const selectedProjectProducts = selectedProject
    ? products.filter((product) => product.projectId === selectedProject.projectId)
    : [];
  const selectedProjectEdgeIds = new Set(
    selectedProject
      ? bindings
          .filter((binding) => binding.projectId === selectedProject.projectId)
          .map((binding) => binding.edgeId)
      : [],
  );
  const selectedProjectEdges = edgeNodes.filter((edge) =>
    selectedProjectEdgeIds.has(edge.edgeId),
  );

  const updateProject = (patch: Partial<ProjectDefinition>) => {
    if (!projectDraft) return;
    setProjectDraft({ ...projectDraft, ...patch });
    setSaveState('idle');
    setActionError('');
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>项目管理</h2>
          <p>隔离产品配置与边端实例。</p>
        </div>
        <button
          className="primary-button"
          onClick={async () => {
            setSaveState('saving');
            setActionError('');
            try {
              const created = await onCreateProject();
              setSelectedProjectId(created.projectId);
              setProjectDraft(created);
              setSaveState('saved');
            } catch (error) {
              setSaveState('error');
              setActionError(displayError(error));
            }
          }}
          type="button"
        >
          新建项目
        </button>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>项目列表</h3>
          <span>{projects.length} 个项目</span>
        </div>
        <div className="table-wrap">
          <table className="ops-table">
            <thead>
              <tr>
                <th>项目 ID</th>
                <th>项目名称</th>
                <th>环境</th>
                <th>负责人</th>
                <th>产品</th>
                <th>边端</th>
                <th>说明</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {projects.length === 0 ? (
                <tr>
                  <td className="table-empty" colSpan={8}>
                    尚未创建项目。使用右上角“新建项目”建立第一个配置空间。
                  </td>
                </tr>
              ) : null}
              {projects.map((project) => {
                const countProducts = products.filter(
                  (product) => product.projectId === project.projectId,
                ).length;
                const countEdges = bindings.filter(
                  (binding) => binding.projectId === project.projectId,
                ).length;
                return (
                  <tr key={project.projectId}>
                    <td>{project.projectId}</td>
                    <td>{project.projectName}</td>
                    <td>{project.environment}</td>
                    <td>{project.owner}</td>
                    <td>{countProducts}</td>
                    <td>{countEdges}</td>
                    <td>{project.description}</td>
                    <td>
                      <div className="row-actions">
                        <button
                          className="secondary-button compact"
                          onClick={() => {
                            setSelectedProjectId(project.projectId);
                            setProjectDraft(project);
                            setSaveState('idle');
                            setActionError('');
                          }}
                          type="button"
                        >
                          详情
                        </button>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </section>

      {selectedProject && projectDraft ? (
        <Modal
          onClose={() => {
            setSelectedProjectId(undefined);
            setProjectDraft(undefined);
          }}
        >
          <section
            aria-label="项目详情"
            className="modal-panel detail-modal"
            role="dialog"
          >
            <div className="modal-header">
              <div>
                <h3>项目详情 {projectDraft.projectName}</h3>
                <p>{selectedProject.projectId}</p>
              </div>
              <button
                aria-label="关闭"
                className="icon-button"
                onClick={() => {
                  setSelectedProjectId(undefined);
                  setProjectDraft(undefined);
                }}
                type="button"
              >
                ×
              </button>
            </div>
            <div className="form-grid">
              <TemplateTextField
                label="项目名称"
                onChange={(projectNameValue) =>
                  updateProject({ projectName: projectNameValue })
                }
                value={projectDraft.projectName}
              />
              <TemplateTextField
                label="环境"
                onChange={(environment) => updateProject({ environment })}
                value={projectDraft.environment}
              />
              <TemplateTextField
                label="负责人"
                onChange={(owner) => updateProject({ owner })}
                value={projectDraft.owner}
              />
              <TemplateTextField
                label="说明"
                onChange={(description) => updateProject({ description })}
                value={projectDraft.description}
              />
            </div>
            <section className="detail-section">
              <h4>项目资源</h4>
              <div className="template-preview-flow">
                <PreviewStep title="产品" value={`${selectedProjectProducts.length} 个`} />
                <PreviewStep title="边端" value={`${selectedProjectEdges.length} 个`} />
                <PreviewStep
                  title="在线"
                  value={`${selectedProjectEdges.filter((edge) => edge.status === '健康').length} 个`}
                />
                <PreviewStep
                  title="配置版本"
                  value={bindings.find((binding) => binding.projectId === selectedProject.projectId)?.desiredVersion ?? '-'}
                />
              </div>
            </section>
            <div className="drawer-footer">
              <span className="editor-status" role="status">
                {saveState === 'saved'
                  ? '已保存'
                  : saveState === 'deleted'
                    ? '已删除'
                    : saveState === 'saving'
                      ? '保存中'
                      : saveState === 'error'
                        ? actionError
                        : '有未保存修改'}
              </span>
              <button
                className="secondary-button"
                onClick={() => {
                  setSelectedProjectId(undefined);
                  setProjectDraft(undefined);
                }}
                type="button"
              >
                关闭
              </button>
              <button
                className="primary-button"
                disabled={saveState === 'saving'}
                onClick={async () => {
                  setSaveState('saving');
                  setActionError('');
                  try {
                    const saved = await onSaveProject(
                      selectedProject.projectId,
                      projectDraft,
                    );
                    setProjectDraft(saved);
                    setSaveState('saved');
                  } catch (error) {
                    setSaveState('error');
                    setActionError(displayError(error));
                  }
                }}
                type="button"
              >
                保存
              </button>
              <button
                className="danger-button"
                disabled={
                  projects.length <= 1 ||
                  products.some((product) => product.projectId === selectedProject.projectId)
                }
                onClick={async () => {
                  setSaveState('saving');
                  setActionError('');
                  try {
                    await onDeleteProject(selectedProject.projectId);
                    setSelectedProjectId(undefined);
                    setProjectDraft(undefined);
                    setSaveState('deleted');
                  } catch (error) {
                    setSaveState('error');
                    setActionError(displayError(error));
                  }
                }}
                type="button"
              >
                删除
              </button>
            </div>
          </section>
        </Modal>
      ) : null}
    </div>
  );
}

const PRODUCT_CONFIG_TABS: Array<{ key: ProductConfigTab; label: string }> = [
  { key: 'basic', label: '基础信息' },
  { key: 'points', label: '绑定点位' },
  { key: 'collection', label: '采集编排' },
  { key: 'release', label: '发布策略' },
];

function ProductManagementPage({
  dataConfigs = [],
  onCreateTemplate,
  onDeleteTemplate,
  onPublishVersion,
  onRollbackVersion,
  onSaveTemplate,
  pointSets = [],
  projects,
  templates,
  versions = {},
}: {
  dataConfigs?: DataConfigResponse[];
  onCreateTemplate: () => Promise<EdgeTemplateDefinition>;
  onDeleteTemplate: (templateId: EdgeTemplateId) => Promise<void>;
  onPublishVersion: (
    productId: string,
    version: string,
  ) => Promise<ProductVersionResponse>;
  onRollbackVersion: (
    productId: string,
    version: string,
  ) => Promise<ProductVersionResponse>;
  onSaveTemplate: (
    templateId: EdgeTemplateId,
    nextTemplate: EdgeTemplateDefinition,
  ) => Promise<EdgeTemplateDefinition>;
  pointSets?: PointSetResponse[];
  projects: ProjectDefinition[];
  templates: EdgeTemplateDefinition[];
  versions?: Record<string, ProductVersionResponse[]>;
}) {
  const [selectedTemplateId, setSelectedTemplateId] = useState<EdgeTemplateId>();
  const [templateDraft, setTemplateDraft] = useState<EdgeTemplateDefinition>();
  const [activeProductTab, setActiveProductTab] = useState<ProductConfigTab>('basic');
  const [saveState, setSaveState] = useState<
    'idle' | 'saving' | 'saved' | 'deleted' | 'error'
  >('idle');
  const [actionError, setActionError] = useState('');
  const persistedTemplate = templates.find(
    (template) => template.templateId === selectedTemplateId,
  );
  const selectedTemplate =
    templateDraft?.templateId === selectedTemplateId
      ? templateDraft
      : persistedTemplate;
  const selectedVersions = selectedTemplate
    ? [...(versions[selectedTemplate.templateId] ?? [])].sort((left, right) =>
        right.createdAt.localeCompare(left.createdAt),
      )
    : [];

  const updateTemplate = (patch: Partial<EdgeTemplateDefinition>) => {
    if (!selectedTemplate) return;
    const nextTemplate = { ...selectedTemplate, ...patch };
    setTemplateDraft(nextTemplate);
    setSaveState('idle');
    setActionError('');
  };

  const updateDataConfig = (
    patch: Partial<EdgeTemplateDefinition['dataConfig']>,
  ) => {
    if (!selectedTemplate) return;
    updateTemplate({
      dataConfig: {
        ...selectedTemplate.dataConfig,
        ...patch,
      },
    });
  };

  const updatePoint = (
    pointId: string,
    patch: Partial<CreatePointMappingRequest & { pointId: string }>,
  ) => {
    if (!selectedTemplate) return;
    const nextPoints = selectedTemplate.points.map((point) =>
      point.pointId === pointId ? { ...point, ...patch } : point,
    );
    const nextDataPoints = selectedTemplate.dataConfig.points.map((point) =>
      point.pointId === pointId
        ? {
            ...point,
            addressKind: patch.addressKind ?? point.addressKind,
            addressValue: patch.addressValue ?? point.addressValue,
            semanticId: patch.semanticId ?? point.semanticId,
            unit: patch.unit ?? point.unit,
            valueType: patch.valueType ?? point.valueType,
          }
        : point,
    );
    updateTemplate({
      points: nextPoints,
      dataConfig: { ...selectedTemplate.dataConfig, points: nextDataPoints },
    });
  };

  const addPoint = () => {
    if (!selectedTemplate) return;
    const nextIndex = selectedTemplate.points.length + 1;
    const pointId = `point_${nextIndex}`;
    updateTemplate({
      points: [
        ...selectedTemplate.points,
        {
          addressKind: 'holding_register',
          addressValue: String(40000 + nextIndex * 2),
          deviceId: selectedTemplate.task.deviceId,
          intervalMs: selectedTemplate.task.intervalMs,
          pointId,
          semanticId: pointId,
          unit: '',
          valueType: 'float32',
        },
      ],
      dataConfig: {
        ...selectedTemplate.dataConfig,
        points: [
          ...selectedTemplate.dataConfig.points,
          {
            addressKind: 'holding_register',
            addressValue: String(40000 + nextIndex * 2),
            jsonField: pointId,
            pointId,
            semanticId: pointId,
            unit: '',
            valueType: 'float32',
          },
        ],
      },
      task: {
        ...selectedTemplate.task,
        pointIds: [...selectedTemplate.task.pointIds, pointId],
      },
    });
  };

  const deletePoint = (pointId: string) => {
    if (!selectedTemplate) return;
    updateTemplate({
      points: selectedTemplate.points.filter((point) => point.pointId !== pointId),
      dataConfig: {
        ...selectedTemplate.dataConfig,
        points: selectedTemplate.dataConfig.points.filter((point) => point.pointId !== pointId),
      },
      task: {
        ...selectedTemplate.task,
        pointIds: selectedTemplate.task.pointIds.filter((id) => id !== pointId),
      },
    });
  };

  const bindPointSetResource = (pointSet: PointSetResponse) => {
    if (!selectedTemplate) return;
    const newTemplatePoints = pointSet.points
      .filter((point) => !selectedTemplate.points.some((item) => item.pointId === point.pointId))
      .map((point) => ({
        addressKind: point.address.kind,
        addressValue: point.address.value,
        deviceId: selectedTemplate.task.deviceId,
        intervalMs: point.intervalMs,
        pointId: point.pointId,
        semanticId: point.semanticId,
        unit: point.unit ?? '',
        valueType: point.valueType,
      }));
    const newDataPoints = newTemplatePoints.map((point) => ({
      addressKind: point.addressKind,
      addressValue: point.addressValue,
      jsonField: point.pointId,
      pointId: point.pointId,
      semanticId: point.semanticId,
      unit: point.unit,
      valueType: point.valueType,
    }));
    updateTemplate({
      pointSetIds: Array.from(
        new Set([...(selectedTemplate.pointSetIds ?? []), pointSet.pointSetId]),
      ),
      points: [...selectedTemplate.points, ...newTemplatePoints],
      dataConfig: {
        ...selectedTemplate.dataConfig,
        points: mergeDataConfigPoints(selectedTemplate.dataConfig.points, newDataPoints),
      },
      task: {
        ...selectedTemplate.task,
        pointIds: Array.from(
          new Set([
            ...selectedTemplate.task.pointIds,
            ...newTemplatePoints.map((point) => point.pointId),
          ]),
        ),
      },
    });
  };

  const unbindPointSetResource = (pointSet: PointSetResponse) => {
    if (!selectedTemplate) return;
    const remainingPointSetIds = (selectedTemplate.pointSetIds ?? []).filter(
      (pointSetId) => pointSetId !== pointSet.pointSetId,
    );
    const retainedPointIds = new Set(
      pointSets
        .filter((candidate) => remainingPointSetIds.includes(candidate.pointSetId))
        .flatMap((candidate) => candidate.points.map((point) => point.pointId)),
    );
    const removedPointIds = pointSet.points
      .map((point) => point.pointId)
      .filter((pointId) => !retainedPointIds.has(pointId));
    updateTemplate({
      pointSetIds: remainingPointSetIds,
      points: selectedTemplate.points.filter((point) => !removedPointIds.includes(point.pointId)),
      dataConfig: {
        ...selectedTemplate.dataConfig,
        points: selectedTemplate.dataConfig.points.filter(
          (point) => !removedPointIds.includes(point.pointId),
        ),
      },
      task: {
        ...selectedTemplate.task,
        pointIds: selectedTemplate.task.pointIds.filter(
          (pointId) => !removedPointIds.includes(pointId),
        ),
      },
    });
  };

  const bindCollectionPipeline = (config: DataConfigResponse) => {
    if (!selectedTemplate) return;
    const boundPoints = config.points.map((point) => ({
      addressKind: point.addressKind,
      addressValue: point.addressValue,
      connectionId: config.protocolConnectionId,
      deviceId: config.deviceId,
      intervalMs: config.collection.periodMs,
      pointId: point.pointId,
      semanticId: point.semanticId,
      unit: point.unit ?? '',
      valueType: point.valueType,
    }));
    updateTemplate({
      dataConfig: {
        collection: config.collection,
        configId: config.configId,
        deviceId: config.deviceId,
        enabled: config.enabled,
        name: config.name,
        points: config.points,
        publish: config.publish,
      },
      points: mergeTemplatePoints(selectedTemplate.points, boundPoints),
      task: {
        ...selectedTemplate.task,
        deviceId: config.deviceId,
        intervalMs: config.collection.periodMs,
        pointIds: config.points.map((point) => point.pointId),
      },
    });
  };

  const runVersionAction = async (
    action: 'publish' | 'rollback',
    version: string,
  ) => {
    if (!selectedTemplate) return;
    setSaveState('saving');
    setActionError('');
    try {
      if (action === 'publish') {
        await onPublishVersion(selectedTemplate.templateId, version);
      } else {
        await onRollbackVersion(selectedTemplate.templateId, version);
      }
      setTemplateDraft(undefined);
      setSaveState('saved');
    } catch (error) {
      setSaveState('error');
      setActionError(displayError(error));
    }
  };

  return (
    <div className="page-stack">
      <section className="page-intro">
        <div>
          <h2>产品管理</h2>
          <p>产品统一维护点位资源、采集编排和上报策略，边端只绑定产品并继承版本化配置。</p>
        </div>
        <button
          className="primary-button"
          disabled={saveState === 'saving' || projects.length === 0}
          onClick={async () => {
            setSaveState('saving');
            setActionError('');
            try {
              const created = await onCreateTemplate();
              setSelectedTemplateId(created.templateId);
              setTemplateDraft(created);
              setSaveState('saved');
            } catch (error) {
              setSaveState('error');
              setActionError(displayError(error));
            }
          }}
          title={projects.length === 0 ? '请先创建项目' : undefined}
          type="button"
        >
          新建产品
        </button>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h3>产品列表</h3>
          <span>{templates.length} 个产品</span>
        </div>
        <div className="table-wrap">
          <table className="ops-table">
            <thead>
              <tr>
                <th>产品 ID</th>
                <th>产品名称</th>
                <th>项目</th>
                <th>类型</th>
                <th>版本</th>
                <th>版本状态</th>
                <th>协议</th>
                <th>绑定点位</th>
                <th>采集流水线</th>
                <th>输出 Topic</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {templates.length === 0 ? (
                <tr>
                  <td className="table-empty" colSpan={11}>
                    {projects.length === 0
                      ? '请先在项目管理中创建项目，再新建产品。'
                      : '尚未创建产品。新建产品后可绑定点位集并配置采集编排。'}
                  </td>
                </tr>
              ) : null}
              {templates.map((template) => (
                <tr key={template.templateId}>
                  <td>{template.templateId}</td>
                  <td>{template.name}</td>
                  <td>{projectName(projects, template.projectId)}</td>
                  <td>{template.productType}</td>
                  <td>{template.version}</td>
                  <td>{productVersionStatusText(versions[template.templateId] ?? [])}</td>
                  <td>{template.connection.protocolType}</td>
                  <td>{template.points.length} 个点位</td>
                  <td>{template.dataConfig.name}</td>
                  <td>{template.dataConfig.publish.topicTemplate}</td>
                  <td>
                    <div className="row-actions">
                      <button
                        className="secondary-button compact"
                        onClick={() => {
                          setSelectedTemplateId(template.templateId);
                          setTemplateDraft(template);
                          setActiveProductTab('basic');
                          setSaveState('idle');
                          setActionError('');
                        }}
                        type="button"
                      >
                        配置
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      {selectedTemplate ? (
        <Modal
          onClose={() => {
            setSelectedTemplateId(undefined);
            setTemplateDraft(undefined);
          }}
        >
          <section
            aria-label="产品配置"
            className="modal-panel detail-modal product-config-modal"
            role="dialog"
          >
            <div className="modal-header">
              <div>
                <h3>产品配置 {selectedTemplate.name}</h3>
                <p>
                  {selectedTemplate.templateId} · {projectName(projects, selectedTemplate.projectId)} ·
                  {selectedTemplate.version}
                </p>
              </div>
              <button
                aria-label="关闭"
                className="icon-button"
                onClick={() => {
                  setSelectedTemplateId(undefined);
                  setTemplateDraft(undefined);
                }}
                type="button"
              >
                ×
              </button>
            </div>

            <nav className="workspace-tabs product-config-tabs" aria-label="产品配置标签" role="tablist">
              {PRODUCT_CONFIG_TABS.map((tab) => (
                <button
                  aria-selected={activeProductTab === tab.key}
                  className={activeProductTab === tab.key ? 'workspace-tab active' : 'workspace-tab'}
                  key={tab.key}
                  onClick={() => setActiveProductTab(tab.key)}
                  role="tab"
                  type="button"
                >
                  {tab.label}
                </button>
              ))}
            </nav>

            <section className="workspace-tab-panel product-config-panel" role="tabpanel">
              {activeProductTab === 'basic' ? (
                <section className="detail-section product-tab-section">
                  <div className="product-section-title">
                    <h4>基础信息</h4>
                    <span>决定边端绑定后展示的产品身份和版本</span>
                  </div>
                  <div className="form-grid">
                    <label className="editor-control">
                      <span>所属项目</span>
                      <select
                        aria-label="所属项目"
                        value={selectedTemplate.projectId}
                        onChange={(event) => updateTemplate({ projectId: event.target.value })}
                      >
                        {projects.map((project) => (
                          <option key={project.projectId} value={project.projectId}>
                            {project.projectName}
                          </option>
                        ))}
                      </select>
                    </label>
                    <TemplateTextField
                      label="产品名称"
                      onChange={(name) => updateTemplate({ name })}
                      value={selectedTemplate.name}
                    />
                    <TemplateTextField
                      label="产品类型"
                      onChange={(productType) => updateTemplate({ productType })}
                      value={selectedTemplate.productType}
                    />
                    <TemplateTextField
                      label="产品版本"
                      onChange={(version) => updateTemplate({ version })}
                      value={selectedTemplate.version}
                    />
                    <TemplateTextField
                      label="适用场景"
                      onChange={(recommendedFor) => updateTemplate({ recommendedFor })}
                      value={selectedTemplate.recommendedFor}
                    />
                    <TemplateTextField
                      label="标签"
                      onChange={(value) =>
                        updateTemplate({
                          highlights: value
                            .split(',')
                            .map((item) => item.trim())
                            .filter(Boolean),
                        })
                      }
                      value={selectedTemplate.highlights.join(', ')}
                    />
                    <TemplateTextArea
                      label="说明"
                      onChange={(description) => updateTemplate({ description })}
                      value={selectedTemplate.description}
                    />
                  </div>
                </section>
              ) : null}
              {activeProductTab === 'points' ? (
                <section className="detail-section product-tab-section">
                  <div className="product-section-title">
                    <div>
                      <h4>绑定点位资源</h4>
                      <span>从点位管理选择可复用输入点位，产品只保存引用关系</span>
                    </div>
                  </div>
                  <ProductPointBindingList
                    boundPointSetIds={selectedTemplate.pointSetIds ?? []}
                    onBindSet={bindPointSetResource}
                    onUnbindSet={unbindPointSetResource}
                    pointSets={pointSets.filter(
                      (pointSet) => pointSet.projectId === selectedTemplate.projectId,
                    )}
                  />
                </section>
              ) : null}
              {activeProductTab === 'collection' ? (
                <section className="detail-section product-tab-section">
                  <div className="product-section-title">
                    <div>
                      <h4>采集编排</h4>
                      <span>自动使用当前产品绑定点位，拖拽节点并连线形成点位输入、计算节点与 MQTT 输出链路</span>
                    </div>
                  </div>
                  <ProductCollectionPlanner
                    onChange={(dataConfig) => updateTemplate({ dataConfig })}
                    template={selectedTemplate}
                  />
                </section>
              ) : null}
              {activeProductTab === 'release' ? (
                <section className="detail-section product-tab-section">
                  <div className="product-section-title">
                    <h4>发布策略</h4>
                    <span>产品配置保存后形成版本，边端绑定产品后从 Cloud 拉取对应版本</span>
                  </div>
                  <div className="product-release-summary">
                    <PreviewStep title="版本" value={selectedTemplate.version} />
                    <PreviewStep title="协议" value={selectedTemplate.connection.protocolType} />
                    <PreviewStep title="点位" value={`${selectedTemplate.points.length} 个点位`} />
                    <PreviewStep title="采集" value={`${selectedTemplate.task.intervalMs}ms`} />
                    <PreviewStep title="上报" value={selectedTemplate.dataConfig.publish.topicTemplate} />
                  </div>
                  <div className="table-wrap product-version-table">
                    <table className="ops-table">
                      <thead>
                        <tr>
                          <th>版本</th>
                          <th>状态</th>
                          <th>点位集</th>
                          <th>采集配置</th>
                          <th>创建时间</th>
                          <th>操作</th>
                        </tr>
                      </thead>
                      <tbody>
                        {selectedVersions.map((version) => (
                          <tr key={version.version}>
                            <td>{version.version}</td>
                            <td>
                              <span className={`tag ${version.status === 'published' ? 'ok' : version.status === 'draft' ? 'warn' : ''}`}>
                                {productVersionStatusLabel(version.status)}
                              </span>
                            </td>
                            <td>{version.pointSetIds.length} 个</td>
                            <td>{version.dataConfigs.length} 套</td>
                            <td>{formatCatalogTime(version.createdAt)}</td>
                            <td>
                              {version.status === 'draft' ? (
                                <button
                                  className="primary-button compact"
                                  disabled={saveState === 'saving'}
                                  onClick={() => void runVersionAction('publish', version.version)}
                                  type="button"
                                >
                                  发布此版本
                                </button>
                              ) : null}
                              {version.status === 'retired' ? (
                                <button
                                  className="secondary-button compact"
                                  disabled={saveState === 'saving'}
                                  onClick={() => void runVersionAction('rollback', version.version)}
                                  type="button"
                                >
                                  回滚到此版本
                                </button>
                              ) : null}
                              {version.status === 'published' ? <span>当前版本</span> : null}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </section>
              ) : null}
            </section>

            <div className="drawer-footer">
              <span className="editor-status" role="status">
                {saveState === 'saved'
                  ? '已保存'
                  : saveState === 'deleted'
                    ? '已删除'
                    : saveState === 'saving'
                      ? '保存中'
                      : saveState === 'error'
                        ? actionError
                        : '有未保存修改'}
              </span>
              <button
                className="secondary-button"
                onClick={() => {
                  setSelectedTemplateId(undefined);
                  setTemplateDraft(undefined);
                }}
                type="button"
              >
                关闭
              </button>
              <button
                className="primary-button"
                disabled={saveState === 'saving'}
                onClick={async () => {
                  setSaveState('saving');
                  setActionError('');
                  try {
                    const saved = await onSaveTemplate(
                      selectedTemplate.templateId,
                      selectedTemplate,
                    );
                    setTemplateDraft(saved);
                    setSaveState('saved');
                  } catch (error) {
                    setSaveState('error');
                    setActionError(displayError(error));
                  }
                }}
                type="button"
              >
                保存
              </button>
              <button
                className="danger-button"
                disabled={templates.length <= 1}
                onClick={async () => {
                  setSaveState('saving');
                  setActionError('');
                  try {
                    await onDeleteTemplate(selectedTemplate.templateId);
                    setSelectedTemplateId(undefined);
                    setTemplateDraft(undefined);
                    setSaveState('deleted');
                  } catch (error) {
                    setSaveState('error');
                    setActionError(displayError(error));
                  }
                }}
                type="button"
              >
                删除
              </button>
            </div>
          </section>
        </Modal>
      ) : null}
    </div>
  );
}

function productVersionStatusLabel(
  status: ProductVersionResponse['status'],
): string {
  if (status === 'draft') return '草稿';
  if (status === 'published') return '已发布';
  return '已退役';
}

function productVersionStatusText(versions: ProductVersionResponse[]): string {
  const draft = versions.find((version) => version.status === 'draft');
  if (draft) return `${draft.version} 草稿`;
  const published = versions.find((version) => version.status === 'published');
  return published ? `${published.version} 已发布` : '未发布';
}

function formatCatalogTime(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : date.toLocaleString('zh-CN', { hour12: false });
}

interface ProductAlgorithmDefinition {
  category: string;
  defaultParams: Record<string, boolean | number | string | string[]>;
  description: string;
  kind: string;
  label: string;
}

const PRODUCT_SCENARIO_ALGORITHMS: ProductAlgorithmDefinition[] = [
  {
    category: '聚合',
    defaultParams: { metrics: ['avg', 'min', 'max', 'sum', 'count'], windowMs: 5000 },
    description: '按时间窗口输出均值、极值、总和、计数、首末值',
    kind: 'window_aggregate',
    label: '窗口聚合',
  },
  {
    category: '聚合',
    defaultParams: { windowMs: 5000 },
    description: '对连续采样计算滑动平均值',
    kind: 'moving_average',
    label: '移动平均',
  },
  {
    category: '聚合',
    defaultParams: { metrics: ['avg', 'min', 'max', 'sum', 'count', 'first', 'last'], windowMs: 10000 },
    description: '输出完整窗口统计结果',
    kind: 'statistics',
    label: '统计汇总',
  },
  {
    category: '过滤',
    defaultParams: { threshold: 0 },
    description: '数值变化超过阈值才进入输出',
    kind: 'change_report',
    label: '变化上报',
  },
  {
    category: '过滤',
    defaultParams: { threshold: 0.1 },
    description: '过滤小范围抖动，降低无效消息',
    kind: 'deadband_filter',
    label: '死区过滤',
  },
  {
    category: '过滤',
    defaultParams: { stableMs: 1000 },
    description: '输入稳定指定时间后才输出',
    kind: 'debounce',
    label: '信号防抖',
  },
  {
    category: '规则',
    defaultParams: { durationMs: 5000, operator: 'Gt', threshold: 0 },
    description: '条件持续满足指定时间后输出一次，条件复位后可再次触发',
    kind: 'duration_condition',
    label: '持续条件',
  },
  {
    category: '转换',
    defaultParams: { expression: 'p0', outputField: 'value' },
    description: '支持四则运算及 min/max/abs/round/sqrt/pow',
    kind: 'expression',
    label: '表达式计算',
  },
  {
    category: '转换',
    defaultParams: { factor: 1, offset: 0 },
    description: '按 factor × value + offset 做工程量换算',
    kind: 'scale_offset',
    label: '缩放偏移',
  },
  {
    category: '转换',
    defaultParams: { max: 100, min: 0 },
    description: '将数值限制在指定上下限内',
    kind: 'clamp',
    label: '数值限幅',
  },
  {
    category: '转换',
    defaultParams: { perMs: 1000 },
    description: '计算单位时间内的数值变化率',
    kind: 'rate_of_change',
    label: '变化率',
  },
  {
    category: '结构',
    defaultParams: {},
    description: '多路点位汇合后交给下游计算或输出',
    kind: 'merge_points',
    label: '多点合并',
  },
  {
    category: '路由',
    defaultParams: { operator: 'Gt', threshold: 0 },
    description: '按条件将数据送往命中或未命中分支',
    kind: 'condition_route',
    label: '条件分支',
  },
  {
    category: '事件',
    defaultParams: { operator: 'Gt', severity: 'Warning', threshold: 0 },
    description: '满足阈值条件时输出事件',
    kind: 'alarm_event',
    label: '告警事件',
  },
];

function ProductCollectionPlanner({
  onChange,
  template,
}: {
  onChange: (dataConfig: EdgeTemplateDefinition['dataConfig']) => void;
  template: EdgeTemplateDefinition;
}) {
  const [selectedNodeId, setSelectedNodeId] = useState<string>('mqtt-output');
  const [connectFrom, setConnectFrom] = useState<{ nodeId: string; portId: string }>();
  const [dragWire, setDragWire] = useState<{ fromNodeId: string; fromPort: string; x: number; y: number }>();
  const [draggingNodeId, setDraggingNodeId] = useState<string>();
  const [nodeMenu, setNodeMenu] = useState<{ nodeId: string; x: number; y: number }>();
  const [editingNodeId, setEditingNodeId] = useState<string>();
  const wireDragRef = useRef<{
    fromNodeId: string;
    fromPort: string;
    moved: boolean;
    pointerId: number;
    startX: number;
    startY: number;
  } | undefined>(undefined);
  const mouseWireDragRef = useRef<{
    fromNodeId: string;
    fromPort: string;
    moved: boolean;
    startX: number;
    startY: number;
  } | undefined>(undefined);
  const suppressPortClickRef = useRef(false);
  const selectedPointIds = new Set(template.dataConfig.points.map((point) => point.pointId));
  const graph = buildProductPlannerGraph(
    template,
    template.dataConfig,
    template.dataConfig.visualGraph,
  );
  const effectiveAlgorithmIds = graph.nodes
    .filter((node) => node.kind === 'algorithm' || node.kind === 'json')
    .map((node) => node.refId ?? node.nodeId);
  const selectedAlgorithmIds = new Set(effectiveAlgorithmIds);
  const graphDiagnostics = buildProductRuntimeDiagnostics(graph);
  const graphIssueCount = Object.values(graphDiagnostics).reduce(
    (count, items) => count + items.length,
    0,
  );
  const selectedNode = graph.nodes.find((node) => node.nodeId === selectedNodeId) ?? graph.nodes[0];
  const editingNode = graph.nodes.find((node) => node.nodeId === editingNodeId);
  const updateDataConfig = (nextConfig: EdgeTemplateDefinition['dataConfig']) => {
    onChange({
      ...nextConfig,
      visualGraph: buildProductPlannerGraph(template, nextConfig, nextConfig.visualGraph),
    });
  };

  const updateGraph = (visualGraph: DataConfigVisualGraph) => {
    onChange({
      ...template.dataConfig,
      visualGraph: buildProductPlannerGraph(template, template.dataConfig, visualGraph),
    });
  };

  const addPointNode = (point: CreatePointMappingRequest & { pointId: string }) => {
    const pointData = templatePointToDataConfigPoint(point);
    const points = mergeDataConfigPoints(template.dataConfig.points, [pointData]);
    const nextGraph = buildProductPlannerGraph(
      template,
      { ...template.dataConfig, points },
      template.dataConfig.visualGraph,
    );
    setSelectedNodeId(`point-${point.pointId}`);
    updateDataConfig({
      ...template.dataConfig,
      points,
      visualGraph: nextGraph,
    });
  };

  const addAlgorithmNode = (kind: string) => {
    const nodeId = nextProductComputeNodeId(graph, kind);
    const instanceIndex = graph.nodes.filter(
      (node) => (node.kind === 'algorithm' || node.kind === 'json') && node.refId === kind,
    ).length + 1;
    const nextGraph: DataConfigVisualGraph = {
      edges: graph.edges,
      nodes: [
        ...graph.nodes,
        {
          kind: 'algorithm',
          label: productComputeInstanceLabel(kind, instanceIndex),
          nodeId,
          params: defaultProductComputeParams(kind, template.dataConfig.collection.periodMs),
          refId: kind,
          x: 360,
          y: 110 + graph.nodes.filter((node) => node.kind === 'algorithm').length * 92,
        },
      ],
    };
    setSelectedNodeId(nodeId);
    onChange({
      ...template.dataConfig,
      algorithmIds: [...effectiveAlgorithmIds, kind],
      visualGraph: nextGraph,
    });
  };

  const removePoint = (pointId: string) => {
    if (selectedNodeId === `point-${pointId}`) setSelectedNodeId('mqtt-output');
    if (editingNodeId === `point-${pointId}`) setEditingNodeId(undefined);
    updateDataConfig({
      ...template.dataConfig,
      points: template.dataConfig.points.filter((point) => point.pointId !== pointId),
    });
  };

  const removeAlgorithmNode = (nodeId: string) => {
    const nextGraph = {
      edges: graph.edges.filter((edge) => edge.from !== nodeId && edge.to !== nodeId),
      nodes: graph.nodes.filter((node) => node.nodeId !== nodeId),
    };
    if (selectedNodeId === nodeId) setSelectedNodeId('mqtt-output');
    if (editingNodeId === nodeId) setEditingNodeId(undefined);
    onChange({
      ...template.dataConfig,
      algorithmIds: nextGraph.nodes
        .filter((node) => node.kind === 'algorithm' || node.kind === 'json')
        .map((node) => node.refId ?? node.nodeId),
      visualGraph: nextGraph,
    });
  };

  const deleteGraphNode = (node: DataConfigVisualGraph['nodes'][number]) => {
    if (node.kind === 'point' && node.refId) {
      removePoint(node.refId);
      return;
    }
    if (node.kind === 'algorithm' && node.refId) {
      removeAlgorithmNode(node.nodeId);
      return;
    }
    if (node.kind === 'mqtt' && graph.nodes.filter((item) => item.kind === 'mqtt').length > 1) {
      const nextNodes = graph.nodes.filter((item) => item.nodeId !== node.nodeId);
      updateGraph({
        edges: graph.edges.filter((edge) => edge.from !== node.nodeId && edge.to !== node.nodeId),
        nodes: nextNodes,
      });
      if (selectedNodeId === node.nodeId) setSelectedNodeId(nextNodes[0]?.nodeId ?? 'mqtt-output');
      if (editingNodeId === node.nodeId) setEditingNodeId(undefined);
    }
  };

  const addMqttOutputNode = (position?: { x: number; y: number }) => {
    const outputCount = graph.nodes.filter((node) => node.kind === 'mqtt').length;
    const nodeId = `mqtt-output-${Date.now()}`;
    const topicTemplate = `factory/{edge_id}/{device_id}/telemetry-${outputCount + 1}`;
    updateGraph({
      edges: graph.edges,
      nodes: [
        ...graph.nodes,
        {
          kind: 'mqtt',
          label: `MQTT 输出 ${outputCount + 1}`,
          nodeId,
          refId: topicTemplate,
          x: position?.x ?? 680,
          y: position?.y ?? 160 + outputCount * 96,
        },
      ],
    });
    setSelectedNodeId(nodeId);
    setEditingNodeId(nodeId);
  };

  const updateMqttOutputNode = (nodeId: string, values: { label?: string; topic?: string }) => {
    const nextGraph = {
      edges: graph.edges,
      nodes: graph.nodes.map((node) =>
        node.nodeId === nodeId
          ? {
              ...node,
              label: values.label ?? node.label,
              refId: values.topic ?? node.refId,
            }
          : node,
      ),
    };
    const nextConfig =
      nodeId === 'mqtt-output' && values.topic !== undefined
        ? {
            ...template.dataConfig,
            publish: {
              ...template.dataConfig.publish,
              topicTemplate: values.topic,
            },
          }
        : template.dataConfig;
    onChange({ ...nextConfig, visualGraph: nextGraph });
  };

  const updateComputeNodeKind = (nodeId: string, kind: string) => {
    const instanceIndex = graph.nodes.filter(
      (node) =>
        node.nodeId !== nodeId &&
        (node.kind === 'algorithm' || node.kind === 'json') &&
        node.refId === kind,
    ).length + 1;
    const nextGraph: DataConfigVisualGraph = {
      edges: graph.edges,
      nodes: graph.nodes.map((node) =>
        node.nodeId === nodeId
          ? {
              ...node,
              label: productComputeInstanceLabel(kind, instanceIndex),
              params: defaultProductComputeParams(kind, template.dataConfig.collection.periodMs),
              refId: kind,
            }
          : node,
      ),
    };
    onChange({
      ...template.dataConfig,
      algorithmIds: nextGraph.nodes
        .filter((node) => node.kind === 'algorithm' || node.kind === 'json')
        .map((node) => node.refId ?? node.nodeId),
      visualGraph: nextGraph,
    });
  };

  const updateComputeNodeParam = (
    nodeId: string,
    key: string,
    value: boolean | number | string | string[],
  ) => {
    updateGraph({
      edges: graph.edges,
      nodes: graph.nodes.map((node) =>
        node.nodeId === nodeId
          ? { ...node, params: { ...(node.params ?? {}), [key]: value } }
          : node,
      ),
    });
  };

  const updatePointField = (pointId: string, jsonField: string) => {
    updateDataConfig({
      ...template.dataConfig,
      points: template.dataConfig.points.map((point) =>
        point.pointId === pointId ? { ...point, jsonField } : point,
      ),
    });
  };

  const updateNodePosition = (nodeId: string, x: number, y: number) => {
    updateGraph({
      edges: graph.edges,
      nodes: graph.nodes.map((node) =>
        node.nodeId === nodeId ? { ...node, x: Math.max(16, x), y: Math.max(16, y) } : node,
      ),
    });
  };

  const createEdge = (from: string, to: string, requestedFromPort?: string) => {
    if (
      !isAllowedProductGraphEdge(graph, from, to) ||
      wouldCreateProductGraphCycle(graph, from, to)
    ) {
      setConnectFrom(undefined);
      return;
    }
    const fromPort = requestedFromPort ?? defaultProductGraphPort(graph, from, 'out');
    const toPort = defaultProductGraphPort(graph, to, 'in');
    const edgeId = `${from}:${fromPort}-to-${to}:${toPort}`;
    updateGraph({
      edges: graph.edges.some((edge) => edge.edgeId === edgeId)
        ? graph.edges
        : [...graph.edges, { edgeId, from, fromPort, to, toPort }],
      nodes: graph.nodes,
    });
    setConnectFrom(undefined);
  };

  const removeEdge = (edgeId: string) => {
    updateGraph({
      edges: graph.edges.filter((edge) => edge.edgeId !== edgeId),
      nodes: graph.nodes,
    });
  };

  const handleNodeClick = (nodeId: string) => {
    setNodeMenu(undefined);
    if (connectFrom && connectFrom.nodeId !== nodeId) {
      createEdge(connectFrom.nodeId, nodeId, connectFrom.portId);
      setSelectedNodeId(nodeId);
      return;
    }
    setSelectedNodeId(nodeId);
  };

  const handleNodeContextMenu = (
    event: MouseEvent<HTMLButtonElement>,
    node: DataConfigVisualGraph['nodes'][number],
  ) => {
    event.preventDefault();
    event.stopPropagation();
    setSelectedNodeId(node.nodeId);
    setConnectFrom(undefined);
    setDraggingNodeId(undefined);
    const canvas = event.currentTarget.closest('.node-red-canvas');
    const rect = canvas?.getBoundingClientRect();
    setNodeMenu({
      nodeId: node.nodeId,
      x: Math.max(12, event.clientX - (rect?.left ?? 0) + (canvas?.scrollLeft ?? 0)),
      y: Math.max(12, event.clientY - (rect?.top ?? 0) + (canvas?.scrollTop ?? 0)),
    });
  };

  const handleOutputPortClick = (nodeId: string, portId: string) => {
    if (suppressPortClickRef.current) {
      suppressPortClickRef.current = false;
      return;
    }
    setSelectedNodeId(nodeId);
    setNodeMenu(undefined);
    setConnectFrom((current) =>
      current?.nodeId === nodeId && current.portId === portId ? undefined : { nodeId, portId },
    );
  };

  const handleOutputPortPointerDown = (
    event: PointerEvent<HTMLButtonElement>,
    nodeId: string,
    fromPort: string,
  ) => {
    if (event.button !== 0) return;
    event.stopPropagation();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    wireDragRef.current = {
      fromNodeId: nodeId,
      fromPort,
      moved: false,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
    };
    setSelectedNodeId(nodeId);
    setNodeMenu(undefined);
  };

  const handleOutputPortPointerMove = (event: PointerEvent<HTMLButtonElement>) => {
    const drag = wireDragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    if (!drag.moved && Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY) < 5) {
      return;
    }
    drag.moved = true;
    const canvas = event.currentTarget.closest('.node-red-canvas');
    if (!(canvas instanceof HTMLElement)) return;
    const rect = canvas.getBoundingClientRect();
    setConnectFrom({ nodeId: drag.fromNodeId, portId: drag.fromPort });
    setDragWire({
      fromNodeId: drag.fromNodeId,
      fromPort: drag.fromPort,
      x: event.clientX - rect.left + canvas.scrollLeft,
      y: event.clientY - rect.top + canvas.scrollTop,
    });
  };

  const finishOutputPortDrag = (event: PointerEvent<HTMLButtonElement>, cancelled = false) => {
    const drag = wireDragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    event.stopPropagation();
    event.currentTarget.releasePointerCapture?.(event.pointerId);
    wireDragRef.current = undefined;

    if (drag.moved) {
      const target = cancelled
        ? undefined
        : document
            .elementFromPoint(event.clientX, event.clientY)
            ?.closest<HTMLElement>('[data-node-input]');
      const targetNodeId = target?.dataset.nodeInput;
      if (targetNodeId && targetNodeId !== drag.fromNodeId) {
        createEdge(drag.fromNodeId, targetNodeId, drag.fromPort);
      } else {
        setConnectFrom(undefined);
      }
      setDragWire(undefined);
      suppressPortClickRef.current = true;
      window.setTimeout(() => {
        suppressPortClickRef.current = false;
      }, 0);
    }
  };

  const handleOutputPortMouseDown = (
    event: MouseEvent<HTMLButtonElement>,
    nodeId: string,
    fromPort: string,
  ) => {
    if (event.button !== 0) return;
    event.stopPropagation();
    mouseWireDragRef.current = {
      fromNodeId: nodeId,
      fromPort,
      moved: false,
      startX: event.clientX,
      startY: event.clientY,
    };
    setSelectedNodeId(nodeId);
    setNodeMenu(undefined);
  };

  const handleCanvasMouseMove = (event: MouseEvent<HTMLDivElement>) => {
    const drag = mouseWireDragRef.current;
    if (!drag) return;
    if (!drag.moved && Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY) < 5) {
      return;
    }
    drag.moved = true;
    const rect = event.currentTarget.getBoundingClientRect();
    setConnectFrom({ nodeId: drag.fromNodeId, portId: drag.fromPort });
    setDragWire({
      fromNodeId: drag.fromNodeId,
      fromPort: drag.fromPort,
      x: event.clientX - rect.left + event.currentTarget.scrollLeft,
      y: event.clientY - rect.top + event.currentTarget.scrollTop,
    });
  };

  const finishCanvasMouseDrag = (event: MouseEvent<HTMLDivElement>) => {
    const drag = mouseWireDragRef.current;
    if (!drag) return;
    mouseWireDragRef.current = undefined;
    if (!drag.moved) return;
    const target = document
      .elementFromPoint(event.clientX, event.clientY)
      ?.closest<HTMLElement>('[data-node-input]');
    const targetNodeId = target?.dataset.nodeInput;
    if (targetNodeId && targetNodeId !== drag.fromNodeId) {
      createEdge(drag.fromNodeId, targetNodeId, drag.fromPort);
    } else {
      setConnectFrom(undefined);
    }
    setDragWire(undefined);
    suppressPortClickRef.current = true;
    window.setTimeout(() => {
      suppressPortClickRef.current = false;
    }, 0);
  };

  const handleInputPortClick = (nodeId: string) => {
    setSelectedNodeId(nodeId);
    setNodeMenu(undefined);
    if (!connectFrom || connectFrom.nodeId === nodeId) return;
    createEdge(connectFrom.nodeId, nodeId, connectFrom.portId);
  };

  const handleCanvasDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    const raw = event.dataTransfer.getData('application/x-edgeops-product-node');
    const rect = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - rect.left - 80;
    const y = event.clientY - rect.top - 32;
    if (raw.startsWith('point:')) {
      const pointId = raw.replace('point:', '');
      const point = template.points.find((item) => item.pointId === pointId);
      if (point) {
        const pointData = templatePointToDataConfigPoint(point);
        const points = mergeDataConfigPoints(template.dataConfig.points, [pointData]);
        const nodeId = `point-${point.pointId}`;
        const nextGraph = buildProductPlannerGraph(
          template,
          { ...template.dataConfig, points },
          template.dataConfig.visualGraph,
        );
        updateDataConfig({
          ...template.dataConfig,
          points,
          visualGraph: {
            edges: nextGraph.edges,
            nodes: nextGraph.nodes.map((node) => (node.nodeId === nodeId ? { ...node, x, y } : node)),
          },
        });
        setSelectedNodeId(nodeId);
      }
      return;
    }
    if (raw.startsWith('algorithm:')) {
      const kind = raw.replace('algorithm:', '');
      const nodeId = nextProductComputeNodeId(graph, kind);
      const instanceIndex = graph.nodes.filter(
        (node) => (node.kind === 'algorithm' || node.kind === 'json') && node.refId === kind,
      ).length + 1;
      const nextGraph: DataConfigVisualGraph = {
        edges: graph.edges,
        nodes: [
          ...graph.nodes,
          {
            kind: 'algorithm',
            label: productComputeInstanceLabel(kind, instanceIndex),
            nodeId,
            params: defaultProductComputeParams(kind, template.dataConfig.collection.periodMs),
            refId: kind,
            x,
            y,
          },
        ],
      };
      onChange({
        ...template.dataConfig,
        algorithmIds: [...effectiveAlgorithmIds, kind],
        visualGraph: nextGraph,
      });
      setSelectedNodeId(nodeId);
      return;
    }
    if (raw === 'mqtt-output') {
      addMqttOutputNode({ x, y });
    }
  };

  return (
    <div
      className={
        editingNode
          ? 'product-flow-designer node-red-flow-designer has-inspector'
          : 'product-flow-designer node-red-flow-designer canvas-only'
      }
    >
      <aside className="product-flow-palette" aria-label="采集编排资源">
        <div>
          <h5>产品点位</h5>
          {template.points.map((point) => (
            <button
              className={selectedPointIds.has(point.pointId) ? 'selected' : ''}
              draggable
              key={point.pointId}
              onClick={() => addPointNode(point)}
              onDragStart={(event) => {
                event.dataTransfer.setData('application/x-edgeops-product-node', `point:${point.pointId}`);
              }}
              type="button"
            >
              <strong>{point.pointId}</strong>
              <span>{point.semanticId ?? point.addressValue}</span>
            </button>
          ))}
        </div>
        <div>
          <h5>计算节点</h5>
          {PRODUCT_SCENARIO_ALGORITHMS.map((algorithm) => (
            <button
              className={selectedAlgorithmIds.has(algorithm.kind) ? 'selected' : ''}
              draggable
              key={algorithm.kind}
              onClick={() => addAlgorithmNode(algorithm.kind)}
              onDragStart={(event) => {
                event.dataTransfer.setData('application/x-edgeops-product-node', `algorithm:${algorithm.kind}`);
              }}
              type="button"
            >
              <strong>{algorithm.label}</strong>
              <span>{algorithm.description}</span>
            </button>
          ))}
        </div>
        <div>
          <h5>输出节点</h5>
          <button
            draggable
            onClick={() => addMqttOutputNode()}
            onDragStart={(event) => {
              event.dataTransfer.setData('application/x-edgeops-product-node', 'mqtt-output');
            }}
            type="button"
          >
            <strong>MQTT 输出</strong>
            <span>拖入画布创建独立主题</span>
          </button>
        </div>
        <details className="node-red-flow-settings">
          <summary>流程设置</summary>
          <div className="node-red-flow-settings-body">
            <label className="editor-control">
              <span>流水线 ID</span>
              <input
                aria-label="流水线 ID"
                onChange={(event) =>
                  onChange({ ...template.dataConfig, configId: event.target.value })
                }
                value={template.dataConfig.configId}
              />
            </label>
            <label className="editor-control">
              <span>名称</span>
              <input
                aria-label="流水线名称"
                onChange={(event) =>
                  onChange({ ...template.dataConfig, name: event.target.value })
                }
                value={template.dataConfig.name}
              />
            </label>
            <label className="editor-control">
              <span>采集周期(ms)</span>
              <input
                aria-label="采集周期(ms)"
                min={1}
                onChange={(event) =>
                  onChange({
                    ...template.dataConfig,
                    collection: {
                      ...template.dataConfig.collection,
                      periodMs: Number(event.target.value),
                    },
                  })
                }
                type="number"
                value={template.dataConfig.collection.periodMs}
              />
            </label>
          </div>
        </details>
      </aside>
      <section className="node-red-canvas-shell" aria-label="采集编排画布">
        <div className="node-red-toolbar">
          <div>
            <strong>{template.dataConfig.name}</strong>
            <span>
              {template.dataConfig.points.length} 点位 / {effectiveAlgorithmIds.length} 计算节点 /{' '}
              {graph.nodes.filter((node) => node.kind === 'mqtt').length} 输出
            </span>
          </div>
          <div className="node-red-toolbar-actions">
            <span className={graphIssueCount === 0 ? 'flow-health ok' : 'flow-health warn'}>
              {graphIssueCount === 0 ? '拓扑有效' : `${graphIssueCount} 项待连接`}
            </span>
            {connectFrom ? (
              <button
                className="secondary-button compact active"
                onClick={() => setConnectFrom(undefined)}
                type="button"
              >
                取消连线
              </button>
            ) : null}
            <button
              className="secondary-button compact"
              onClick={() => updateGraph(layoutProductPlannerGraph(graph))}
              type="button"
            >
              自动布局
            </button>
          </div>
        </div>
        <div
          className={connectFrom ? 'node-red-canvas is-connecting' : 'node-red-canvas'}
          onClick={() => setNodeMenu(undefined)}
          onContextMenu={(event) => {
            event.preventDefault();
            setNodeMenu(undefined);
          }}
          onDragOver={(event) => event.preventDefault()}
          onDrop={handleCanvasDrop}
          onMouseMove={handleCanvasMouseMove}
          onMouseUp={finishCanvasMouseDrag}
        >
          <svg aria-hidden="true" className="node-red-wires">
            <defs>
              <marker
                id="node-red-arrow"
                markerHeight="8"
                markerUnits="userSpaceOnUse"
                markerWidth="8"
                orient="auto"
                refX="7"
                refY="4"
                viewBox="0 0 8 8"
              >
                <path className="node-red-arrow-head" d="M 1.2 1.2 L 6.8 4 L 1.2 6.8" />
              </marker>
              <marker
                id="node-red-arrow-selected"
                markerHeight="8"
                markerUnits="userSpaceOnUse"
                markerWidth="8"
                orient="auto"
                refX="7"
                refY="4"
                viewBox="0 0 8 8"
              >
                <path className="node-red-arrow-head selected" d="M 1.2 1.2 L 6.8 4 L 1.2 6.8" />
              </marker>
            </defs>
            {graph.edges.map((edge) => {
              const from = graph.nodes.find((node) => node.nodeId === edge.from);
              const to = graph.nodes.find((node) => node.nodeId === edge.to);
              if (!from || !to) return null;
              const fromPoint = getProductNodeAnchor(from, 'out', edge.fromPort ?? undefined);
              const toPoint = getProductNodeAnchor(to, 'in');
              const isSelectedEdge = Boolean(
                selectedNode && (edge.from === selectedNode.nodeId || edge.to === selectedNode.nodeId),
              );
              return (
                <g key={edge.edgeId}>
                  <path
                    className={isSelectedEdge ? 'selected' : ''}
                    d={makeProductWirePath(fromPoint.x, fromPoint.y, toPoint.x, toPoint.y)}
                    markerEnd={isSelectedEdge ? 'url(#node-red-arrow-selected)' : 'url(#node-red-arrow)'}
                  />
                  <circle
                    aria-label={`删除连线 ${edge.edgeId}`}
                    className="node-red-wire-hit"
                    cx={(fromPoint.x + toPoint.x) / 2}
                    cy={(fromPoint.y + toPoint.y) / 2}
                    onClick={() => removeEdge(edge.edgeId)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter' || event.key === ' ') removeEdge(edge.edgeId);
                    }}
                    r={7}
                    role="button"
                    tabIndex={0}
                  >
                    <title>{`删除连线 ${edge.edgeId}`}</title>
                  </circle>
                </g>
              );
            })}
            {dragWire ? (() => {
              const from = graph.nodes.find((node) => node.nodeId === dragWire.fromNodeId);
              if (!from) return null;
              const fromPoint = getProductNodeAnchor(from, 'out', dragWire.fromPort);
              return (
                <path
                  className="node-red-wire-preview"
                  d={makeProductWirePath(fromPoint.x, fromPoint.y, dragWire.x, dragWire.y)}
                  markerEnd="url(#node-red-arrow-selected)"
                />
              );
            })() : null}
          </svg>
          {graph.nodes.map((node) => (
            <div
              className={[
                'node-red-node',
                `kind-${node.kind}`,
                selectedNode?.nodeId === node.nodeId ? 'selected' : '',
                connectFrom?.nodeId === node.nodeId ? 'connecting' : '',
                connectFrom && connectFrom.nodeId !== node.nodeId ? 'connect-target' : '',
              ].join(' ')}
              key={node.nodeId}
              style={{ left: node.x, top: node.y }}
            >
              <button
                aria-label={`流程节点 ${node.label}`}
                className="node-red-node-body"
                onClick={() => handleNodeClick(node.nodeId)}
                onDoubleClick={() => {
                  setSelectedNodeId(node.nodeId);
                  setEditingNodeId(node.nodeId);
                  setNodeMenu(undefined);
                }}
                onContextMenu={(event) => handleNodeContextMenu(event, node)}
                onPointerDown={(event) => {
                  event.currentTarget.setPointerCapture(event.pointerId);
                  setDraggingNodeId(node.nodeId);
                }}
                onPointerMove={(event: PointerEvent<HTMLButtonElement>) => {
                  if (draggingNodeId !== node.nodeId) return;
                  const canvas = event.currentTarget.closest('.node-red-canvas');
                  if (!canvas) return;
                  const rect = canvas.getBoundingClientRect();
                  updateNodePosition(
                    node.nodeId,
                    event.clientX - rect.left - 80,
                    event.clientY - rect.top - 30,
                  );
                }}
                onPointerUp={(event) => {
                  event.currentTarget.releasePointerCapture(event.pointerId);
                  setDraggingNodeId(undefined);
                }}
                type="button"
              >
                <span>{productNodeKindLabel(node.kind)}</span>
                <strong>{node.label}</strong>
                <small>{node.refId ?? 'payload'}</small>
              </button>
              {node.kind !== 'point' ? (
                <button
                  aria-label={`连接到 ${node.label}`}
                  className="node-red-port in"
                  data-node-input={node.nodeId}
                  onClick={(event) => {
                    event.stopPropagation();
                    handleInputPortClick(node.nodeId);
                  }}
                  onKeyDown={(event) => {
                    if (event.key !== 'Enter' && event.key !== ' ') return;
                    event.preventDefault();
                    event.stopPropagation();
                    handleInputPortClick(node.nodeId);
                  }}
                  onPointerDown={(event) => event.stopPropagation()}
                  type="button"
                />
              ) : null}
              {node.kind !== 'mqtt'
                ? productNodeOutputPorts(node).map((port, portIndex, ports) => (
                    <button
                      aria-label={
                        ports.length === 1
                          ? `从 ${node.label} 连线`
                          : `从 ${node.label} 的 ${port.label} 端口连线`
                      }
                      className={`node-red-port out port-${port.id}`}
                      data-port-label={ports.length > 1 ? port.label : undefined}
                      key={port.id}
                      onClick={(event) => {
                        event.stopPropagation();
                        handleOutputPortClick(node.nodeId, port.id);
                      }}
                      onKeyDown={(event) => {
                        if (event.key !== 'Enter' && event.key !== ' ') return;
                        event.preventDefault();
                        event.stopPropagation();
                        handleOutputPortClick(node.nodeId, port.id);
                      }}
                      onMouseDown={(event) => handleOutputPortMouseDown(event, node.nodeId, port.id)}
                      onPointerCancel={(event) => finishOutputPortDrag(event, true)}
                      onPointerDown={(event) => handleOutputPortPointerDown(event, node.nodeId, port.id)}
                      onPointerMove={handleOutputPortPointerMove}
                      onPointerUp={finishOutputPortDrag}
                      style={{ top: `calc(${((portIndex + 1) / (ports.length + 1)) * 100}% - 13px)` }}
                      title={port.label}
                      type="button"
                    />
                  ))
                : null}
            </div>
          ))}
          {connectFrom ? (
            <div className="node-red-hint">
              正在连接“{productGraphNodeName(graph, connectFrom.nodeId)} / {productPortLabel(connectFrom.portId)}”，点击或拖到目标输入端口
            </div>
          ) : null}
          {nodeMenu ? (
            <div
              className="node-red-context-menu"
              role="menu"
              style={{ left: nodeMenu.x, top: nodeMenu.y }}
            >
              <button
                onClick={(event) => {
                  event.stopPropagation();
                  setSelectedNodeId(nodeMenu.nodeId);
                  setEditingNodeId(nodeMenu.nodeId);
                  setNodeMenu(undefined);
                }}
                role="menuitem"
                type="button"
              >
                编辑节点
              </button>
              <button
                disabled={
                  graph.nodes.find((node) => node.nodeId === nodeMenu.nodeId)?.kind === 'mqtt' &&
                  graph.nodes.filter((node) => node.kind === 'mqtt').length <= 1
                }
                onClick={(event) => {
                  event.stopPropagation();
                  const node = graph.nodes.find((item) => item.nodeId === nodeMenu.nodeId);
                  if (node) deleteGraphNode(node);
                  if (editingNodeId === nodeMenu.nodeId) setEditingNodeId(undefined);
                  setNodeMenu(undefined);
                }}
                role="menuitem"
                type="button"
              >
                删除节点
              </button>
            </div>
          ) : null}
        </div>
      </section>
      {editingNode ? (
        <aside aria-label="节点编辑" className="product-flow-inspector node-red-edit-inspector">
          <div className="node-red-inspector-title">
            <div>
              <span>编辑{productNodeKindLabel(editingNode.kind)}节点</span>
              <h5>{editingNode.label}</h5>
            </div>
            <button
              aria-label="关闭节点编辑"
              className="icon-button compact-icon"
              onClick={() => setEditingNodeId(undefined)}
              title="关闭"
              type="button"
            >
              <X aria-hidden="true" size={16} />
            </button>
          </div>
          <div className="node-red-inspector-body">
            {editingNode.kind === 'point' ? (
              <section className="node-red-inspector-section">
                <div className="node-red-section-heading">
                  <h4>字段映射</h4>
                  <p>设置该点位进入后续计算时使用的字段名。</p>
                </div>
                <label className="editor-control">
                  <span>输出字段</span>
                  <input
                    aria-label={`产品 JSON 字段 ${editingNode.refId}`}
                    onChange={(event) => {
                      if (editingNode.refId) updatePointField(editingNode.refId, event.target.value);
                    }}
                    value={
                      template.dataConfig.points.find((point) => point.pointId === editingNode.refId)?.jsonField ?? ''
                    }
                  />
                </label>
              </section>
            ) : null}
            {editingNode.kind === 'algorithm' ? (
              <section className="node-red-inspector-section">
                <div className="node-red-section-heading">
                  <h4>计算设置</h4>
                </div>
                <label className="editor-control">
                  <span>计算类型</span>
                  <select
                    aria-label="选中计算类型"
                    value={editingNode.refId ?? ''}
                    onChange={(event) => updateComputeNodeKind(editingNode.nodeId, event.target.value)}
                  >
                    {PRODUCT_SCENARIO_ALGORITHMS.map((algorithm) => (
                      <option key={algorithm.kind} value={algorithm.kind}>
                        {algorithm.label}
                      </option>
                    ))}
                  </select>
                </label>
                <p className="node-red-control-help">
                  {productAlgorithmDescription(editingNode.refId ?? '')}
                </p>
                {hasProductComputeParameters(editingNode.refId ?? '') ? (
                  <div className="node-red-parameter-block">
                    <ProductComputeParameterEditor
                      node={editingNode}
                      onParamChange={(key, value) =>
                        updateComputeNodeParam(editingNode.nodeId, key, value)
                      }
                    />
                  </div>
                ) : null}
              </section>
            ) : null}
            {editingNode.kind === 'mqtt' ? (
              <section className="node-red-inspector-section">
                <div className="node-red-section-heading">
                  <h4>输出设置</h4>
                  <p>配置该分支独立发布的主题和传输等级。</p>
                </div>
                <label className="editor-control">
                  <span>输出名称</span>
                  <input
                    aria-label="MQTT 输出名称"
                    onChange={(event) =>
                      updateMqttOutputNode(editingNode.nodeId, { label: event.target.value })
                    }
                    value={editingNode.label}
                  />
                </label>
                <label className="editor-control">
                  <span>MQTT Topic</span>
                  <input
                    aria-label="MQTT Topic"
                    onChange={(event) =>
                      updateMqttOutputNode(editingNode.nodeId, { topic: event.target.value })
                    }
                    value={editingNode.refId ?? ''}
                  />
                </label>
                <label className="editor-control">
                  <span>QoS</span>
                  <select
                    aria-label="MQTT QoS"
                    onChange={(event) =>
                      onChange({
                        ...template.dataConfig,
                        publish: { ...template.dataConfig.publish, qos: Number(event.target.value) },
                      })
                    }
                    value={template.dataConfig.publish.qos}
                  >
                    <option value={0}>QoS 0</option>
                    <option value={1}>QoS 1</option>
                    <option value={2}>QoS 2</option>
                  </select>
                </label>
              </section>
            ) : null}
          </div>
          <div className="node-red-inspector-footer">
            {editingNode.kind !== 'mqtt' || graph.nodes.filter((node) => node.kind === 'mqtt').length > 1 ? (
              <button
                className="danger-button compact node-red-delete-node"
                onClick={() => deleteGraphNode(editingNode)}
                type="button"
              >
                <Trash2 aria-hidden="true" size={14} />
                删除
              </button>
            ) : <span />}
            <button
              className="primary-button compact"
              onClick={() => setEditingNodeId(undefined)}
              type="button"
            >
              <Check aria-hidden="true" size={14} />
              完成
            </button>
          </div>
        </aside>
      ) : null}
    </div>
  );
}

function ProductComputeParameterEditor({
  node,
  onParamChange,
}: {
  node: DataConfigVisualGraph['nodes'][number];
  onParamChange: (key: string, value: boolean | number | string | string[]) => void;
}) {
  const kind = node.refId ?? 'expression';
  const params = node.params ?? {};
  const numberField = (key: string, label: string, fallback: number, min?: number) => (
    <label className="editor-control" key={key}>
      <span>{label}</span>
      <input
        aria-label={label}
        min={min}
        onChange={(event) => onParamChange(key, Number(event.target.value))}
        type="number"
        value={Number(params[key] ?? fallback)}
      />
    </label>
  );

  if (kind === 'window_aggregate' || kind === 'statistics' || kind === 'moving_average') {
    const availableMetrics = kind === 'moving_average'
      ? ['avg']
      : ['avg', 'min', 'max', 'sum', 'count', 'first', 'last'];
    const selectedMetrics = Array.isArray(params.metrics)
      ? params.metrics.map(String)
      : kind === 'moving_average'
        ? ['avg']
        : ['avg', 'min', 'max', 'sum', 'count'];
    return (
      <div className="node-red-parameter-grid">
        {numberField('windowMs', '窗口时长(ms)', 5000, 1)}
        <fieldset className="node-red-metric-picker">
          <legend>输出指标</legend>
          {availableMetrics.map((metric) => (
            <label key={metric}>
              <input
                checked={selectedMetrics.includes(metric)}
                disabled={kind === 'moving_average'}
                onChange={(event) => {
                  const next = event.target.checked
                    ? Array.from(new Set([...selectedMetrics, metric]))
                    : selectedMetrics.filter((item) => item !== metric);
                  onParamChange('metrics', next.length ? next : ['avg']);
                }}
                type="checkbox"
              />
              <span>{metric}</span>
            </label>
          ))}
        </fieldset>
      </div>
    );
  }
  if (kind === 'change_report' || kind === 'deadband_filter') {
    return <div className="node-red-parameter-grid">{numberField('threshold', '变化阈值', kind === 'deadband_filter' ? 0.1 : 0, 0)}</div>;
  }
  if (kind === 'debounce') {
    return <div className="node-red-parameter-grid">{numberField('stableMs', '稳定时长(ms)', 1000, 1)}</div>;
  }
  if (kind === 'scale_offset') {
    return <div className="node-red-parameter-grid">{numberField('factor', '缩放系数', 1)}{numberField('offset', '偏移量', 0)}</div>;
  }
  if (kind === 'clamp') {
    return <div className="node-red-parameter-grid">{numberField('min', '最小值', 0)}{numberField('max', '最大值', 100)}</div>;
  }
  if (kind === 'rate_of_change') {
    return <div className="node-red-parameter-grid">{numberField('perMs', '变化率单位(ms)', 1000, 1)}</div>;
  }
  if (kind === 'expression') {
    return (
      <div className="node-red-parameter-grid">
        <label className="editor-control">
          <span>表达式</span>
          <input
            aria-label="计算表达式"
            onChange={(event) => onParamChange('expression', event.target.value)}
            placeholder="(p0 + p1) / 2"
            value={String(params.expression ?? 'p0')}
          />
        </label>
        <small className="node-red-expression-help">输入别名按连线顺序为 p0、p1…，支持 + - * /、括号及 min/max/abs/round/sqrt/pow。</small>
      </div>
    );
  }
  if (
    kind === 'condition_route' ||
    kind === 'alarm_event' ||
    kind === 'duration_condition'
  ) {
    return (
      <div className="node-red-parameter-grid two-column">
        <label className="editor-control">
          <span>比较符</span>
          <select
            aria-label="条件比较符"
            onChange={(event) => onParamChange('operator', event.target.value)}
            value={String(params.operator ?? 'Gt')}
          >
            <option value="Gt">大于</option>
            <option value="Gte">大于等于</option>
            <option value="Lt">小于</option>
            <option value="Lte">小于等于</option>
            <option value="Eq">等于</option>
            <option value="Ne">不等于</option>
          </select>
        </label>
        {numberField('threshold', '比较阈值', 0)}
        {kind === 'duration_condition'
          ? numberField('durationMs', '持续时长(ms)', 5000, 1)
          : null}
      </div>
    );
  }
  return null;
}

function hasProductComputeParameters(kind: string): boolean {
  const definition = PRODUCT_SCENARIO_ALGORITHMS.find((algorithm) => algorithm.kind === kind);
  return Boolean(definition && Object.keys(definition.defaultParams).length > 0);
}

function templatePointToDataConfigPoint(
  point: CreatePointMappingRequest & { pointId: string },
): DataConfigPoint {
  return {
    addressKind: point.addressKind ?? 'holding_register',
    addressValue: point.addressValue ?? point.pointId,
    jsonField: point.pointId,
    pointId: point.pointId,
    semanticId: point.semanticId ?? point.pointId,
    unit: point.unit ?? '',
    valueType: point.valueType ?? 'float32',
  };
}

function getEffectiveProductAlgorithmIds(dataConfig: EdgeTemplateDefinition['dataConfig']) {
  return dataConfig.algorithmIds?.length ? dataConfig.algorithmIds : ['merge_points'];
}

const PRODUCT_FLOW_NODE_WIDTH = 168;
const PRODUCT_FLOW_NODE_HEIGHT = 66;

function buildProductPlannerGraph(
  _template: EdgeTemplateDefinition,
  dataConfig: EdgeTemplateDefinition['dataConfig'],
  existingGraph?: DataConfigVisualGraph,
): DataConfigVisualGraph {
  const previousNodes = new Map(
    (existingGraph?.nodes ?? []).map((node) => [node.nodeId, node]),
  );
  const pointNodes = dataConfig.points.map((point, index) => ({
    kind: 'point' as const,
    label: point.jsonField || point.pointId,
    nodeId: `point-${point.pointId}`,
    refId: point.pointId,
    x: previousNodes.get(`point-${point.pointId}`)?.x ?? 72,
    y: previousNodes.get(`point-${point.pointId}`)?.y ?? 84 + index * 86,
  }));
  const previousComputeNodes = (existingGraph?.nodes ?? []).filter(
    (node) => node.kind === 'algorithm' || node.kind === 'json',
  );
  const configuredAlgorithmIds = getEffectiveProductAlgorithmIds(dataConfig);
  const algorithmNodes = previousComputeNodes.length
    ? previousComputeNodes.map((node, index) => {
        const kind =
          node.kind === 'json'
            ? 'merge_points'
            : node.refId ?? configuredAlgorithmIds[index] ?? 'merge_points';
        const definition = productAlgorithmDefinition(kind);
        const legacyLabel = node.kind === 'json' || node.label === 'JSON Payload';
        return {
          ...node,
          kind: 'algorithm' as const,
          label: legacyLabel
            ? definition?.label ?? '多点合并'
            : node.label || definition?.label || kind || `计算节点 ${index + 1}`,
          params: {
            ...defaultProductComputeParams(kind, dataConfig.collection.periodMs),
            ...(node.params ?? {}),
          },
          refId: kind,
          x: node.x ?? 360,
          y: node.y ?? 110 + index * 92,
        };
      })
    : getEffectiveProductAlgorithmIds(dataConfig).map((algorithmId, index) => ({
        kind: 'algorithm' as const,
        label:
          PRODUCT_SCENARIO_ALGORITHMS.find((algorithm) => algorithm.kind === algorithmId)?.label ??
          algorithmId,
        nodeId: `algorithm-${algorithmId}`,
        params: defaultProductComputeParams(algorithmId, dataConfig.collection.periodMs),
        refId: algorithmId,
        x: 360,
        y: 110 + index * 92,
      }));
  const previousOutputNodes = (existingGraph?.nodes ?? []).filter((node) => node.kind === 'mqtt');
  const outputNodes = previousOutputNodes.length
    ? previousOutputNodes.map((node, index) => ({
        ...node,
        label: node.label || `MQTT 输出 ${index + 1}`,
        refId:
          node.refId ||
          (node.nodeId === 'mqtt-output'
            ? dataConfig.publish.topicTemplate
            : `factory/{edge_id}/{device_id}/telemetry-${index + 1}`),
      }))
    : [
        {
          kind: 'mqtt' as const,
          label: 'MQTT 输出 1',
          nodeId: 'mqtt-output',
          refId: dataConfig.publish.topicTemplate,
          x: 680,
          y: 160,
        },
      ];
  const nodes = [
    ...pointNodes,
    ...algorithmNodes,
    ...outputNodes,
  ];
  const nodeIds = new Set(nodes.map((node) => node.nodeId));
  const preservedEdges = (existingGraph?.edges ?? []).filter(
    (edge) =>
      nodeIds.has(edge.from) &&
      nodeIds.has(edge.to) &&
      isAllowedProductGraphEdge({ edges: [], nodes }, edge.from, edge.to),
  );
  return { edges: preservedEdges, nodes };
}

function nextProductComputeNodeId(graph: DataConfigVisualGraph, kind: string) {
  const base = `algorithm-${kind}`;
  const ids = new Set(graph.nodes.map((node) => node.nodeId));
  if (!ids.has(base)) return base;
  let index = 2;
  while (ids.has(`${base}-${index}`)) index += 1;
  return `${base}-${index}`;
}

function productComputeInstanceLabel(kind: string, instanceIndex: number) {
  const base =
    PRODUCT_SCENARIO_ALGORITHMS.find((algorithm) => algorithm.kind === kind)?.label ?? kind;
  return instanceIndex > 1 ? `${base} ${instanceIndex}` : base;
}

function productAlgorithmDefinition(kind: string | null | undefined) {
  return PRODUCT_SCENARIO_ALGORITHMS.find((algorithm) => algorithm.kind === kind);
}

function defaultProductComputeParams(kind: string, collectionPeriodMs: number) {
  const defaults: Record<string, boolean | number | string | string[]> =
    productAlgorithmDefinition(kind)?.defaultParams ?? {};
  if ('windowMs' in defaults) {
    return { ...defaults, windowMs: Math.max(Number(defaults.windowMs), collectionPeriodMs) };
  }
  return { ...defaults };
}

function productNodeOutputPorts(node: DataConfigVisualGraph['nodes'][number]) {
  if (node.kind === 'point') return [{ id: 'value', label: '数据' }];
  if (node.refId === 'condition_route') {
    return [
      { id: 'matched', label: '命中' },
      { id: 'unmatched', label: '未命中' },
    ];
  }
  return [{ id: 'output', label: '输出' }];
}

function productPortLabel(portId: string) {
  if (portId === 'matched') return '命中';
  if (portId === 'unmatched') return '未命中';
  if (portId === 'value') return '数据';
  return '输出';
}

function layoutProductPlannerGraph(graph: DataConfigVisualGraph): DataConfigVisualGraph {
  const indexes = new Map<DataConfigVisualGraph['nodes'][number]['kind'], number>();
  return {
    edges: graph.edges,
    nodes: graph.nodes.map((node) => {
      const index = indexes.get(node.kind) ?? 0;
      indexes.set(node.kind, index + 1);
      const x = node.kind === 'point' ? 72 : node.kind === 'mqtt' ? 680 : 370;
      return { ...node, x, y: 84 + index * 104 };
    }),
  };
}

function isAllowedProductGraphEdge(graph: DataConfigVisualGraph, fromId: string, toId: string) {
  if (fromId === toId) return false;
  const from = graph.nodes.find((node) => node.nodeId === fromId);
  const to = graph.nodes.find((node) => node.nodeId === toId);
  if (!from || !to) return false;
  if (from.kind === 'mqtt') return false;
  if (to.kind === 'point') return false;
  if (from.kind === 'point') return to.kind === 'algorithm';
  if (from.kind === 'algorithm') return to.kind === 'algorithm' || to.kind === 'mqtt';
  return false;
}

function wouldCreateProductGraphCycle(
  graph: DataConfigVisualGraph,
  fromId: string,
  toId: string,
) {
  const pending = [toId];
  const visited = new Set<string>();
  while (pending.length > 0) {
    const nodeId = pending.pop();
    if (!nodeId || visited.has(nodeId)) continue;
    if (nodeId === fromId) return true;
    visited.add(nodeId);
    graph.edges
      .filter((edge) => edge.from === nodeId)
      .forEach((edge) => pending.push(edge.to));
  }
  return false;
}

function getProductNodeAnchor(
  node: DataConfigVisualGraph['nodes'][number],
  side: 'in' | 'out',
  portId?: string,
) {
  const ports = productNodeOutputPorts(node);
  const portIndex = Math.max(0, ports.findIndex((port) => port.id === portId));
  return {
    x: node.x + (side === 'out' ? PRODUCT_FLOW_NODE_WIDTH : 0),
    y:
      side === 'out'
        ? node.y + (PRODUCT_FLOW_NODE_HEIGHT * (portIndex + 1)) / (ports.length + 1)
        : node.y + PRODUCT_FLOW_NODE_HEIGHT / 2,
  };
}

function makeProductWirePath(x1: number, y1: number, x2: number, y2: number) {
  const curve = Math.min(140, Math.max(48, Math.abs(x2 - x1) * 0.42));
  return `M ${x1} ${y1} C ${x1 + curve} ${y1}, ${x2 - curve} ${y2}, ${x2} ${y2}`;
}

function productNodeKindLabel(kind: DataConfigVisualGraph['nodes'][number]['kind']) {
  if (kind === 'point') return '点位';
  if (kind === 'algorithm') return '计算';
  if (kind === 'json') return 'JSON';
  return 'MQTT';
}

function productGraphNodeName(graph: DataConfigVisualGraph, nodeId: string) {
  return graph.nodes.find((node) => node.nodeId === nodeId)?.label ?? nodeId;
}

function defaultProductGraphPort(
  graph: DataConfigVisualGraph,
  nodeId: string,
  side: 'in' | 'out',
) {
  const node = graph.nodes.find((item) => item.nodeId === nodeId);
  if (!node) return side === 'in' ? 'input' : 'output';
  if (node.kind === 'point') return side === 'in' ? 'bind' : 'value';
  if (node.kind === 'mqtt') return side === 'in' ? 'payload' : 'published';
  if (side === 'out' && node.refId === 'condition_route') return 'matched';
  return side === 'in' ? 'input' : 'output';
}

function productAlgorithmDescription(algorithmId: string) {
  return productAlgorithmDefinition(algorithmId)?.description ?? '自定义计算节点';
}

function mergeDataConfigPoints(current: DataConfigPoint[], next: DataConfigPoint[]) {
  const merged = new Map<string, DataConfigPoint>();
  current.forEach((point) => merged.set(point.pointId, point));
  next.forEach((point) => merged.set(point.pointId, point));
  return Array.from(merged.values());
}

function buildProductFlowDsl(
  dataConfig: EdgeTemplateDefinition['dataConfig'],
  graph: DataConfigVisualGraph,
) {
  return {
    configId: dataConfig.configId,
    name: dataConfig.name,
    runtimePlan: buildProductRuntimePlan(dataConfig, graph),
    schema: 'edgeops.dataflow.v1',
    visualGraph: {
      edges: graph.edges.map((edge) => ({
        from: edge.from,
        fromPort: edge.fromPort ?? defaultProductGraphPort(graph, edge.from, 'out'),
        to: edge.to,
        toPort: edge.toPort ?? defaultProductGraphPort(graph, edge.to, 'in'),
      })),
      nodes: graph.nodes.map((node) => ({
        id: node.nodeId,
        kind: node.kind,
        label: node.label,
        params: node.params ?? {},
        refId: node.refId,
      })),
    },
  };
}

function buildProductRuntimePlan(
  dataConfig: EdgeTemplateDefinition['dataConfig'],
  graph: DataConfigVisualGraph,
) {
  const pointNodes = graph.nodes.filter((node) => node.kind === 'point');
  const computeNodes = graph.nodes.filter((node) => node.kind === 'algorithm' || node.kind === 'json');
  const outputNodes = graph.nodes.filter((node) => node.kind === 'mqtt');

  return {
    collection: {
      periodMs: dataConfig.collection.periodMs,
      retryCount: dataConfig.collection.retryCount,
      timeoutMs: dataConfig.collection.timeoutMs,
    },
    diagnostics: buildProductRuntimeDiagnostics(graph),
    executionTree: outputNodes.map((node) =>
      buildProductRuntimeTreeNode(dataConfig, graph, node.nodeId, new Set<string>()),
    ),
    inputs: pointNodes.map((node) => {
      const point = productPointByNodeId(dataConfig, node.nodeId);
      return {
        address: point
          ? {
              kind: point.addressKind,
              value: point.addressValue,
            }
          : undefined,
        field: point?.jsonField ?? node.label,
        nodeId: node.nodeId,
        pointId: point?.pointId ?? node.refId ?? node.nodeId,
        semanticId: point?.semanticId ?? node.refId ?? node.nodeId,
        unit: point?.unit ?? undefined,
        valueType: point?.valueType ?? 'unknown',
      };
    }),
    outputs: outputNodes.map((node) => ({
      inputs: graph.edges
        .filter((edge) => edge.to === node.nodeId)
        .map((edge) => buildProductRuntimeInputLink(dataConfig, graph, edge)),
      kind: 'mqtt',
      nodeId: node.nodeId,
      outputs: [
        {
          channelId: dataConfig.publish.sinkId,
          kind: 'mqtt_publish',
          payload: dataConfig.publish.payload,
          portId: 'publish',
          qos: dataConfig.publish.qos,
          topic: node.refId ?? dataConfig.publish.topicTemplate,
        },
      ],
    })),
    planVersion: 'edgeops.runtime.dataflow.v1',
    stages: computeNodes.map((node) => ({
      inputs: graph.edges
        .filter((edge) => edge.to === node.nodeId)
        .map((edge) => buildProductRuntimeInputLink(dataConfig, graph, edge)),
      kind: node.refId ?? node.kind,
      label: node.label,
      nodeId: node.nodeId,
      outputs: graph.edges
        .filter((edge) => edge.from === node.nodeId)
        .map((edge) => buildProductRuntimeOutputLink(dataConfig, graph, edge)),
      params: buildProductRuntimeComputeParams(node, dataConfig),
      type: 'compute',
    })),
  };
}

function buildProductRuntimeTreeNode(
  dataConfig: EdgeTemplateDefinition['dataConfig'],
  graph: DataConfigVisualGraph,
  nodeId: string,
  visited: Set<string>,
  viaEdge?: DataConfigVisualGraph['edges'][number],
): Record<string, unknown> {
  const descriptor = buildProductRuntimeEndpoint(dataConfig, graph, nodeId);
  const link = viaEdge
    ? {
        edgeId: viaEdge.edgeId,
        fromPort: viaEdge.fromPort ?? defaultProductGraphPort(graph, viaEdge.from, 'out'),
        toPort: viaEdge.toPort ?? defaultProductGraphPort(graph, viaEdge.to, 'in'),
      }
    : undefined;
  if (visited.has(nodeId)) {
    return { ...descriptor, cycle: true, inputs: [], link };
  }

  const nextVisited = new Set(visited);
  nextVisited.add(nodeId);
  const inputs = graph.edges
    .filter((edge) => edge.to === nodeId)
    .map((edge) => buildProductRuntimeTreeNode(dataConfig, graph, edge.from, nextVisited, edge));

  if (descriptor.kind === 'compute') {
    return {
      ...descriptor,
      inputs,
      link,
      params: buildProductRuntimeComputeParams(
        graph.nodes.find((node) => node.nodeId === nodeId) ?? {
          kind: 'algorithm',
          label: nodeId,
          nodeId,
          refId: String(descriptor.computeKind ?? ''),
          x: 0,
          y: 0,
        },
        dataConfig,
      ),
    };
  }

  if (descriptor.kind === 'mqtt') {
    const outputNode = graph.nodes.find((node) => node.nodeId === nodeId);
    return {
      ...descriptor,
      inputs,
      link,
      outputs: [
        {
          channelId: dataConfig.publish.sinkId,
          kind: 'mqtt_publish',
          payload: dataConfig.publish.payload,
          portId: 'publish',
          qos: dataConfig.publish.qos,
          topic: outputNode?.refId ?? dataConfig.publish.topicTemplate,
        },
      ],
    };
  }

  return { ...descriptor, inputs, link };
}

function buildProductRuntimeInputLink(
  dataConfig: EdgeTemplateDefinition['dataConfig'],
  graph: DataConfigVisualGraph,
  edge: DataConfigVisualGraph['edges'][number],
) {
  return {
    ...buildProductRuntimeEndpoint(dataConfig, graph, edge.from),
    edgeId: edge.edgeId,
    fromPort: edge.fromPort ?? defaultProductGraphPort(graph, edge.from, 'out'),
    toPort: edge.toPort ?? defaultProductGraphPort(graph, edge.to, 'in'),
  };
}

function buildProductRuntimeOutputLink(
  dataConfig: EdgeTemplateDefinition['dataConfig'],
  graph: DataConfigVisualGraph,
  edge: DataConfigVisualGraph['edges'][number],
) {
  return {
    ...buildProductRuntimeEndpoint(dataConfig, graph, edge.to),
    edgeId: edge.edgeId,
    fromPort: edge.fromPort ?? defaultProductGraphPort(graph, edge.from, 'out'),
    toPort: edge.toPort ?? defaultProductGraphPort(graph, edge.to, 'in'),
  };
}

function buildProductRuntimeEndpoint(
  dataConfig: EdgeTemplateDefinition['dataConfig'],
  graph: DataConfigVisualGraph,
  nodeId: string,
) {
  const node = graph.nodes.find((item) => item.nodeId === nodeId);
  if (!node) {
    return {
      kind: 'unknown',
      nodeId,
    };
  }

  if (node.kind === 'point') {
    const point = productPointByNodeId(dataConfig, node.nodeId);
    return {
      field: point?.jsonField ?? node.label,
      kind: 'point',
      nodeId: node.nodeId,
      pointId: point?.pointId ?? node.refId ?? node.nodeId,
      semanticId: point?.semanticId ?? node.refId ?? node.nodeId,
      valueType: point?.valueType ?? 'unknown',
    };
  }

  if (node.kind === 'algorithm' || node.kind === 'json') {
    return {
      computeKind: node.refId ?? node.kind,
      kind: 'compute',
      label: node.label,
      nodeId: node.nodeId,
      params: node.params ?? {},
    };
  }

  return {
    kind: 'mqtt',
    label: node.label,
    nodeId: node.nodeId,
    topic: node.refId ?? dataConfig.publish.topicTemplate,
  };
}

function productPointByNodeId(
  dataConfig: EdgeTemplateDefinition['dataConfig'],
  nodeId: string,
) {
  return dataConfig.points.find((point) => `point-${point.pointId}` === nodeId);
}

function buildProductRuntimeComputeParams(
  node: DataConfigVisualGraph['nodes'][number],
  dataConfig: EdgeTemplateDefinition['dataConfig'],
) {
  const computeKind = node.refId ?? node.kind;
  const configured = node.params ?? {};
  if (computeKind === 'window_aggregate') {
    return {
      metrics: configured.metrics ?? ['avg', 'min', 'max', 'sum', 'count'],
      windowMs: configured.windowMs ?? Math.max(dataConfig.collection.periodMs, 1000),
    };
  }
  if (computeKind === 'change_report') {
    return {
      compare: 'last_value',
      threshold: configured.threshold ?? 0,
    };
  }
  if (computeKind === 'deadband_filter') {
    return {
      absoluteDeadband: configured.threshold ?? 0.1,
      mode: 'suppress_noise',
    };
  }
  if (computeKind === 'expression') {
    return {
      expression: configured.expression ?? 'p0',
      outputField: configured.outputField ?? 'value',
    };
  }
  if (computeKind === 'alarm_event') {
    return {
      condition: `value ${configured.operator ?? 'Gt'} ${configured.threshold ?? 0}`,
      severity: configured.severity ?? 'Warning',
    };
  }
  if (computeKind === 'moving_average' || computeKind === 'statistics') {
    return {
      metrics: configured.metrics ?? (computeKind === 'moving_average' ? ['avg'] : ['avg', 'min', 'max', 'sum', 'count', 'first', 'last']),
      windowMs: configured.windowMs ?? Math.max(dataConfig.collection.periodMs, 1000),
    };
  }
  if (computeKind === 'debounce') return { stableMs: configured.stableMs ?? 1000 };
  if (computeKind === 'duration_condition') {
    return {
      durationMs: configured.durationMs ?? 5000,
      operator: configured.operator ?? 'Gt',
      threshold: configured.threshold ?? 0,
    };
  }
  if (computeKind === 'scale_offset') return { factor: configured.factor ?? 1, offset: configured.offset ?? 0 };
  if (computeKind === 'clamp') return { min: configured.min ?? 0, max: configured.max ?? 100 };
  if (computeKind === 'rate_of_change') return { perMs: configured.perMs ?? 1000 };
  if (computeKind === 'condition_route') {
    return { operator: configured.operator ?? 'Gt', threshold: configured.threshold ?? 0 };
  }
  return {
    outputMode: dataConfig.publish.payload.mode,
    strategy: 'merge_by_json_field',
  };
}

function buildProductRuntimeDiagnostics(graph: DataConfigVisualGraph) {
  const connectedFrom = new Set(graph.edges.map((edge) => edge.from));
  const connectedTo = new Set(graph.edges.map((edge) => edge.to));
  const pointNodes = graph.nodes.filter((node) => node.kind === 'point');
  const computeNodes = graph.nodes.filter((node) => node.kind === 'algorithm' || node.kind === 'json');
  const outputNodes = graph.nodes.filter((node) => node.kind === 'mqtt');

  return {
    cycleNodes: productGraphCycleNodes(graph),
    computeWithoutInput: computeNodes
      .filter((node) => !connectedTo.has(node.nodeId))
      .map((node) => node.nodeId),
    computeWithoutOutput: computeNodes
      .filter((node) => !connectedFrom.has(node.nodeId))
      .map((node) => node.nodeId),
    outputWithoutInput: outputNodes
      .filter((node) => !connectedTo.has(node.nodeId))
      .map((node) => node.nodeId),
    unconnectedInputs: pointNodes
      .filter((node) => !connectedFrom.has(node.nodeId))
      .map((node) => node.nodeId),
  };
}

function productGraphIssueDetails(graph: DataConfigVisualGraph) {
  const diagnostics = buildProductRuntimeDiagnostics(graph);
  return [
    ...diagnostics.cycleNodes.map(
      (nodeId) => `节点 ${productGraphNodeName(graph, nodeId)} 位于循环链路中`,
    ),
    ...diagnostics.unconnectedInputs.map(
      (nodeId) => `点位 ${productGraphNodeName(graph, nodeId)} 尚未连接计算节点`,
    ),
    ...diagnostics.computeWithoutInput.map(
      (nodeId) => `计算节点 ${productGraphNodeName(graph, nodeId)} 缺少输入`,
    ),
    ...diagnostics.computeWithoutOutput.map(
      (nodeId) => `计算节点 ${productGraphNodeName(graph, nodeId)} 缺少下游`,
    ),
    ...diagnostics.outputWithoutInput.map(
      (nodeId) => `MQTT 输出 ${productGraphNodeName(graph, nodeId)} 缺少输入`,
    ),
  ];
}

function productGraphCycleNodes(graph: DataConfigVisualGraph) {
  const indegree = new Map(graph.nodes.map((node) => [node.nodeId, 0]));
  graph.edges.forEach((edge) => indegree.set(edge.to, (indegree.get(edge.to) ?? 0) + 1));
  const queue = graph.nodes
    .filter((node) => (indegree.get(node.nodeId) ?? 0) === 0)
    .map((node) => node.nodeId);
  const visited = new Set<string>();
  while (queue.length > 0) {
    const nodeId = queue.shift();
    if (!nodeId) break;
    visited.add(nodeId);
    graph.edges
      .filter((edge) => edge.from === nodeId)
      .forEach((edge) => {
        const next = (indegree.get(edge.to) ?? 0) - 1;
        indegree.set(edge.to, next);
        if (next === 0) queue.push(edge.to);
      });
  }
  return graph.nodes
    .filter((node) => !visited.has(node.nodeId))
    .map((node) => node.nodeId);
}

function dominantPointSetValue(values: string[]) {
  const counts = new Map<string, number>();
  values.forEach((value) => counts.set(value, (counts.get(value) ?? 0) + 1));
  return Array.from(counts.entries()).sort((left, right) => right[1] - left[1])[0]?.[0] ?? '-';
}

function ProductPointBindingList({
  boundPointSetIds,
  onBindSet,
  onUnbindSet,
  pointSets,
}: {
  boundPointSetIds: string[];
  onBindSet: (pointSet: PointSetResponse) => void;
  onUnbindSet: (pointSet: PointSetResponse) => void;
  pointSets: PointSetResponse[];
}) {
  return (
    <div className="table-wrap product-config-table">
      <table className="ops-table">
        <thead>
          <tr>
            <th>点位集</th>
            <th>协议</th>
            <th>点位数</th>
            <th>周期</th>
            <th>状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {pointSets.map((pointSet) => {
            const isBound = boundPointSetIds.includes(pointSet.pointSetId);
            return (
              <tr key={pointSet.pointSetId}>
                <td>
                  <strong>{pointSet.name}</strong>
                  <small>{pointSet.points.slice(0, 3).map((point) => point.pointId).join(', ')}</small>
                </td>
                <td>{pointSet.protocol}</td>
                <td>{pointSet.points.length} 个</td>
                <td>
                  {dominantPointSetValue(
                    pointSet.points.map((point) => `${point.intervalMs}ms`),
                  )}
                </td>
                <td>
                  <span className={isBound ? 'tag ok' : 'tag warn'}>
                    {isBound ? '已绑定' : '未绑定'}
                  </span>
                </td>
                <td>
                  <button
                    className={isBound ? 'danger-button compact' : 'secondary-button compact'}
                    onClick={() => {
                      if (isBound) {
                        onUnbindSet(pointSet);
                      } else {
                        onBindSet(pointSet);
                      }
                    }}
                    type="button"
                  >
                    {isBound ? '解除' : '绑定'}
                  </button>
                </td>
              </tr>
            );
          })}
          {pointSets.length === 0 ? (
            <tr>
              <td colSpan={6}>当前项目暂无点位集，请先在点位管理中创建。</td>
            </tr>
          ) : null}
        </tbody>
      </table>
    </div>
  );
}

function ProductPipelineBindingList({
  boundConfigId,
  dataConfigs,
  onBind,
}: {
  boundConfigId: string;
  dataConfigs: DataConfigResponse[];
  onBind: (config: DataConfigResponse) => void;
}) {
  return (
    <div className="table-wrap product-config-table">
      <table className="ops-table">
        <thead>
          <tr>
            <th>流水线 ID</th>
            <th>名称</th>
            <th>输入点位</th>
            <th>算法</th>
            <th>输出 Topic</th>
            <th>状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {dataConfigs.map((config) => {
            const isBound = boundConfigId === config.configId;
            return (
              <tr key={`${config.edgeId}:${config.configId}`}>
                <td>{config.configId}</td>
                <td>{config.name}</td>
                <td>{config.points.length} 个点位</td>
                <td>{config.algorithmIds.length ? config.algorithmIds.join(', ') : '未配置计算'}</td>
                <td>{config.publish.topicTemplate}</td>
                <td>
                  <span className={isBound ? 'tag ok' : 'tag warn'}>
                    {isBound ? '当前产品使用' : '可绑定'}
                  </span>
                </td>
                <td>
                  <button
                    className="secondary-button compact"
                    disabled={isBound}
                    onClick={() => onBind(config)}
                    type="button"
                  >
                    {isBound ? '已绑定' : '绑定'}
                  </button>
                </td>
              </tr>
            );
          })}
          {dataConfigs.length === 0 ? (
            <tr>
              <td colSpan={7}>暂无采集编排，请在产品配置中创建。</td>
            </tr>
          ) : null}
        </tbody>
      </table>
    </div>
  );
}

function mergeTemplatePoints(
  current: Array<CreatePointMappingRequest & { pointId: string }>,
  next: Array<CreatePointMappingRequest & { pointId: string }>,
) {
  const merged = new Map<string, CreatePointMappingRequest & { pointId: string }>();
  current.forEach((point) => merged.set(point.pointId, point));
  next.forEach((point) => merged.set(point.pointId, point));
  return Array.from(merged.values());
}

function ProductPointEditor({
  onDeletePoint,
  onUpdatePoint,
  points,
}: {
  onDeletePoint: (pointId: string) => void;
  onUpdatePoint: (
    pointId: string,
    patch: Partial<CreatePointMappingRequest & { pointId: string }>,
  ) => void;
  points: Array<CreatePointMappingRequest & { pointId: string }>;
}) {
  return (
    <div className="table-wrap product-config-table">
      <table className="ops-table">
        <thead>
          <tr>
            <th>Point ID</th>
            <th>设备</th>
            <th>语义</th>
            <th>地址类型</th>
            <th>地址</th>
            <th>类型</th>
            <th>单位</th>
            <th>周期(ms)</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {points.map((point) => (
            <tr key={point.pointId}>
              <td>
                <InlineTemplateInput
                  ariaLabel={`Point ID ${point.pointId}`}
                  onChange={(value) => onUpdatePoint(point.pointId, { pointId: value })}
                  value={point.pointId}
                />
              </td>
              <td>
                <InlineTemplateInput
                  ariaLabel={`设备 ${point.pointId}`}
                  onChange={(deviceId) => onUpdatePoint(point.pointId, { deviceId })}
                  value={point.deviceId ?? ''}
                />
              </td>
              <td>
                <InlineTemplateInput
                  ariaLabel={`语义 ${point.pointId}`}
                  onChange={(semanticId) => onUpdatePoint(point.pointId, { semanticId })}
                  value={point.semanticId ?? ''}
                />
              </td>
              <td>
                <InlineTemplateInput
                  ariaLabel={`地址类型 ${point.pointId}`}
                  onChange={(addressKind) => onUpdatePoint(point.pointId, { addressKind })}
                  value={point.addressKind ?? ''}
                />
              </td>
              <td>
                <InlineTemplateInput
                  ariaLabel={`地址 ${point.pointId}`}
                  onChange={(addressValue) => onUpdatePoint(point.pointId, { addressValue })}
                  value={point.addressValue ?? ''}
                />
              </td>
              <td>
                <InlineTemplateInput
                  ariaLabel={`类型 ${point.pointId}`}
                  onChange={(valueType) => onUpdatePoint(point.pointId, { valueType })}
                  value={point.valueType ?? ''}
                />
              </td>
              <td>
                <InlineTemplateInput
                  ariaLabel={`单位 ${point.pointId}`}
                  onChange={(unit) => onUpdatePoint(point.pointId, { unit })}
                  value={point.unit ?? ''}
                />
              </td>
              <td>
                <InlineTemplateInput
                  ariaLabel={`周期 ${point.pointId}`}
                  onChange={(intervalMs) =>
                    onUpdatePoint(point.pointId, { intervalMs: Number(intervalMs) })
                  }
                  type="number"
                  value={String(point.intervalMs ?? 1000)}
                />
              </td>
              <td>
                <button
                  className="danger-button compact"
                  onClick={() => onDeletePoint(point.pointId)}
                  type="button"
                >
                  删除
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ProductReportFieldList({ points }: { points: SaveDataConfigRequest['points'] }) {
  return (
    <div className="product-report-fields">
      <h5>JSON 字段映射</h5>
      <div className="table-wrap">
        <table className="ops-table">
          <thead>
            <tr>
              <th>Point ID</th>
              <th>JSON 字段</th>
              <th>语义</th>
              <th>地址</th>
              <th>类型</th>
            </tr>
          </thead>
          <tbody>
            {points.map((point) => (
              <tr key={point.pointId}>
                <td>{point.pointId}</td>
                <td>{point.jsonField}</td>
                <td>{point.semanticId}</td>
                <td>{point.addressKind}:{point.addressValue}</td>
                <td>{point.valueType}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function InlineTemplateInput({
  ariaLabel,
  onChange,
  type = 'text',
  value,
}: {
  ariaLabel: string;
  onChange: (value: string) => void;
  type?: string;
  value: string;
}) {
  return (
    <input
      aria-label={ariaLabel}
      className="inline-template-input"
      onChange={(event) => onChange(event.target.value)}
      type={type}
      value={value}
    />
  );
}

function TemplateTextField({
  label,
  onChange,
  type = 'text',
  value,
}: {
  label: string;
  onChange: (value: string) => void;
  type?: string;
  value: string;
}) {
  return (
    <label className="editor-control">
      <span>{label}</span>
      <input
        aria-label={label}
        onChange={(event) => onChange(event.target.value)}
        type={type}
        value={value}
      />
    </label>
  );
}

function TemplateSelectField({
  label,
  onChange,
  options,
  value,
}: {
  label: string;
  onChange: (value: string) => void;
  options: string[];
  value: string;
}) {
  return (
    <label className="editor-control">
      <span>{label}</span>
      <select
        aria-label={label}
        onChange={(event) => onChange(event.target.value)}
        value={value}
      >
        {options.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </select>
    </label>
  );
}

function TemplateTextArea({
  label,
  onChange,
  value,
}: {
  label: string;
  onChange: (value: string) => void;
  value: string;
}) {
  return (
    <label className="editor-control form-wide">
      <span>{label}</span>
      <textarea
        aria-label={label}
        className="template-textarea"
        onChange={(event) => onChange(event.target.value)}
        value={value}
      />
    </label>
  );
}

function EdgeConfigWorkspace({
  algorithms,
  collectionTasks,
  dataConfigs,
  discoverySuggestions,
  edgeId,
  edges,
  edgeTemplates,
  initialTab,
  mqttUplink,
  productBinding,
  projects,
  onCreateCollectionTask,
  onCreateAlgorithm,
  onCreatePoint,
  onCreateProtocolConnection,
  onApplyEdgeTemplate,
  onAssessAlgorithmRisk,
  onDeleteAlgorithm,
  onDeleteCollectionTask,
  onDeleteDataConfig,
  onDeletePoint,
  onDeleteProtocolConnection,
  onGenerateSchedule,
  onImportPoints,
  onPublish,
  onReleaseDiff,
  onRunDiscovery,
  onSaveCollectionTask,
  onSaveAlgorithm,
  onSaveDataConfig,
  onSaveMqttUplink,
  onSavePoint,
  onSaveProtocolConnection,
  onValidateConfig,
  pointMappings,
  protocolConnections,
  releaseList,
}: {
  algorithms?: AlgorithmResponse[];
  collectionTasks?: CollectionTaskResponse[];
  dataConfigs?: DataConfigResponse[];
  discoverySuggestions?: PointMappingSuggestionResponse[];
  edgeId: string;
  edges?: EdgeNodeResponse[];
  edgeTemplates: EdgeTemplateDefinition[];
  initialTab: EdgeConfigTabKey;
  mqttUplink?: MqttUplinkResponse;
  productBinding?: EdgeProductBinding;
  projects: ProjectDefinition[];
  onCreateCollectionTask: (
    edgeId: string,
    request: CreateCollectionTaskRequest,
  ) => Promise<CollectionTaskResponse>;
  onCreateAlgorithm: (
    edgeId: string,
    request: CreateAlgorithmRequest,
  ) => Promise<AlgorithmResponse>;
  onCreatePoint: (
    edgeId?: string,
    request?: CreatePointMappingRequest,
  ) => Promise<PointMappingResponse>;
  onCreateProtocolConnection: (
    edgeId: string,
    request: CreateProtocolConnectionRequest,
  ) => Promise<ProtocolConnectionResponse>;
  onApplyEdgeTemplate: (
    edgeId: string,
    templateId: EdgeTemplateId,
  ) => Promise<ManagementActionResponse>;
  onAssessAlgorithmRisk: (edgeId: string) => Promise<ManagementActionResponse>;
  onDeleteAlgorithm: (edgeId: string, algorithmId: string) => Promise<void>;
  onDeleteCollectionTask: (edgeId: string, taskId: string) => Promise<void>;
  onDeleteDataConfig: (edgeId: string, configId: string) => Promise<void>;
  onDeletePoint: (edgeId: string, pointId: string) => Promise<void>;
  onDeleteProtocolConnection: (
    edgeId: string,
    connectionId: string,
  ) => Promise<void>;
  onGenerateSchedule: (edgeId: string) => Promise<ManagementActionResponse>;
  onImportPoints: (edgeId: string) => Promise<ManagementActionResponse>;
  onPublish: (edgeId: string) => Promise<void>;
  onReleaseDiff: (edgeId: string) => Promise<ManagementActionResponse>;
  onRunDiscovery: (
    edgeId: string,
    request: RunDiscoveryRequest,
  ) => Promise<DiscoveryReportResponse>;
  onSaveCollectionTask: (
    edgeId: string,
    taskId: string,
    request: SaveCollectionTaskRequest,
  ) => Promise<void>;
  onSaveAlgorithm: (
    edgeId: string,
    algorithmId: string,
    request: SaveAlgorithmRequest,
  ) => Promise<void>;
  onSaveDataConfig: (
    edgeId: string,
    configId: string | null,
    request: SaveDataConfigRequest,
  ) => Promise<void>;
  onSaveMqttUplink: (
    edgeId: string,
    request: MqttUplinkResponse,
  ) => Promise<MqttUplinkResponse>;
  onSavePoint: (
    edgeId: string,
    pointId: string,
    request: SavePointMappingRequest,
  ) => Promise<void>;
  onSaveProtocolConnection: (
    edgeId: string,
    connectionId: string,
    request: SaveProtocolConnectionRequest,
  ) => Promise<void>;
  onValidateConfig: (edgeId?: string) => Promise<ManagementActionResponse>;
  pointMappings?: PointMappingResponse[];
  protocolConnections?: ProtocolConnectionResponse[];
  releaseList?: ReleaseListResponse;
}) {
  const [activeTab, setActiveTab] = useState<EdgeConfigTab>(initialTab);
  const edge = edges?.find((item) => item.edgeId === edgeId);
  const tabs: Array<{ key: EdgeConfigTab; label: string }> = [
    { key: 'versions', label: '产品绑定' },
    { key: 'protocol', label: '协议连接' },
    { key: 'points', label: '点位配置' },
    { key: 'collection', label: '采集任务' },
    { key: 'algorithms', label: '算法配置' },
    { key: 'reports', label: '数据上报' },
    { key: 'mqtt', label: 'MQTT' },
    { key: 'discovery', label: '点位探测' },
    { key: 'release', label: '配置发布' },
  ];

  useEffect(() => {
    setActiveTab(initialTab);
  }, [edgeId, initialTab]);

  return (
    <div className="edge-config-workspace">
      <section className="edge-config-header">
        <div>
          <span>边端配置工作区</span>
          <h2>{edge?.displayName ?? edgeId}</h2>
          <p>{edge?.site ?? '未分组'} · {edgeId} · {edge?.runtimeId ?? 'runtime 未上报'}</p>
        </div>
        <div className="edge-config-status">
          <span className={edge?.status === '健康' ? 'tag ok' : 'tag warn'}>
            {edge?.status ?? '未知'}
          </span>
          <strong>{edge?.resources ?? '-'}</strong>
          <small>{edge?.heartbeat ?? '-'}</small>
        </div>
      </section>

      <nav className="workspace-tabs" aria-label="边端配置标签" role="tablist">
        {tabs.map((tab) => (
          <button
            aria-selected={activeTab === tab.key}
            className={activeTab === tab.key ? 'workspace-tab active' : 'workspace-tab'}
            key={tab.key}
            onClick={() => setActiveTab(tab.key)}
            role="tab"
            type="button"
          >
            {tab.label}
          </button>
        ))}
      </nav>

      <section className="workspace-tab-panel" role="tabpanel">
        {activeTab === 'versions' ? (
          <EdgeConfigVersionPanel
            edgeId={edgeId}
            onApplyTemplate={onApplyEdgeTemplate}
            onPublish={onPublish}
            onReleaseDiff={onReleaseDiff}
            onValidateConfig={onValidateConfig}
            productBinding={productBinding}
            projects={projects}
            releaseList={releaseList}
            templates={edgeTemplates}
          />
        ) : null}
        {activeTab === 'protocol' ? (
          <ProtocolConnectionsPage
            connections={protocolConnections}
            edges={edges}
            embedded
            mode="configure"
            onCreateConnection={onCreateProtocolConnection}
            onDeleteConnection={onDeleteProtocolConnection}
            onSaveConnection={onSaveProtocolConnection}
            onValidateConnection={onValidateConfig}
            selectedEdgeId={edgeId}
          />
        ) : null}
        {activeTab === 'points' ? (
          <PointMappingsPage
            edges={edges}
            embedded
            mode="configure"
            onCreatePoint={onCreatePoint}
            onDeletePoint={onDeletePoint}
            onImportPoints={onImportPoints}
            onSavePoint={onSavePoint}
            onValidateDraft={onValidateConfig}
            points={pointMappings}
            selectedEdgeId={edgeId}
          />
        ) : null}
        {activeTab === 'collection' ? (
          <CollectionTasksPage
            edges={edges}
            embedded
            mode="configure"
            onCreateTask={onCreateCollectionTask}
            onDeleteTask={onDeleteCollectionTask}
            onGenerateSchedule={onGenerateSchedule}
            onSaveTask={onSaveCollectionTask}
            selectedEdgeId={edgeId}
            tasks={collectionTasks}
          />
        ) : null}
        {activeTab === 'algorithms' ? (
          <AlgorithmsPage
            algorithms={algorithms}
            edges={edges}
            embedded
            mode="configure"
            onAssessRisk={onAssessAlgorithmRisk}
            onCreateAlgorithm={onCreateAlgorithm}
            onDeleteAlgorithm={onDeleteAlgorithm}
            onSaveAlgorithm={onSaveAlgorithm}
            selectedEdgeId={edgeId}
          />
        ) : null}
        {activeTab === 'reports' ? (
          <DataConfigsPage
            algorithms={algorithms}
            configs={dataConfigs}
            edges={edges}
            embedded
            mqttUplink={mqttUplink}
            onDeleteConfig={onDeleteDataConfig}
            onSaveConfig={onSaveDataConfig}
            pointMappings={pointMappings}
            protocolConnections={protocolConnections}
            selectedEdgeId={edgeId}
          />
        ) : null}
        {activeTab === 'mqtt' ? (
          <MqttUplinkPage
            onSave={onSaveMqttUplink}
            selectedEdgeId={edgeId}
            uplink={mqttUplink}
          />
        ) : null}
        {activeTab === 'discovery' ? (
          <DiscoveryPage
            onRunDiscovery={onRunDiscovery}
            selectedEdgeId={edgeId}
            suggestions={discoverySuggestions}
          />
        ) : null}
        {activeTab === 'release' ? (
          <ReleasesPage
            edges={edges}
            onPublish={onPublish}
            onShowDiff={onReleaseDiff}
            onValidateRelease={onValidateConfig}
            releaseList={releaseList}
            selectedEdgeId={edgeId}
          />
        ) : null}
      </section>
    </div>
  );
}

function formatReleaseBindingStatus(
  releaseResult: ReleaseListResponse['applyResults'][number] | undefined,
) {
  if (!releaseResult) {
    return '待发布';
  }
  const version =
    releaseResult.reportedVersion && releaseResult.reportedVersion !== '-'
      ? releaseResult.reportedVersion
      : releaseResult.desiredVersion;
  return version && version !== '-' ? `${releaseResult.result} · ${version}` : releaseResult.result;
}
