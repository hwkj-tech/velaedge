import type {
  AgentActionResponse,
  AgentChatRequest,
  AgentChatResponse,
  AgentConversationResponse,
  AgentKnowledgeDocumentResponse,
  AgentProviderStatusResponse,
  AgentProposalResponse,
  AlgorithmResponse,
  AuditRecordResponse,
  AuthStatusResponse,
  BindEdgeProductRequest,
  CollectionTaskResponse,
  CreateAlgorithmRequest,
  CreateAgentProposalRequest,
  CreateCollectionTaskRequest,
  CreateDeviceModelRequest,
  CreateEdgeNodeRequest,
  CreatePointMappingRequest,
  DataConfigResponse,
  DeviceModelResponse,
  DiscoveryReportResponse,
  EdgeNodeResponse,
  EdgeAccessTokenResponse,
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
  SaveAgentKnowledgeDocumentRequest,
  ManagementActionResponse,
  SaveAlgorithmRequest,
  SaveCollectionTaskRequest,
  SaveDataConfigRequest,
  SaveDeviceModelRequest,
  SaveMqttUplinkRequest,
  SavePointMappingRequest,
  SavePointSetRequest,
  SaveProductRequest,
  SaveProductVersionRequest,
  SaveProjectRequest,
  SaveProtocolConnectionRequest,
  CreateProtocolConnectionRequest,
  SummaryResponse,
} from './types';

const apiTokenStorageKey = 'edgeops.apiToken';

export function getApiToken(): string | undefined {
  try {
    return window.sessionStorage.getItem(apiTokenStorageKey) ?? undefined;
  } catch {
    return undefined;
  }
}

export function setApiToken(token?: string): void {
  try {
    const normalized = token?.trim();
    if (normalized) {
      window.sessionStorage.setItem(apiTokenStorageKey, normalized);
    } else {
      window.sessionStorage.removeItem(apiTokenStorageKey);
    }
  } catch {
    // The console remains usable in browsers that disable session storage.
  }
}

export async function fetchAuthStatus(
  fetcher: typeof fetch = fetch,
): Promise<AuthStatusResponse> {
  return requestJson<AuthStatusResponse>('/api/auth/me', fetcher);
}

export async function fetchProjects(
  fetcher: typeof fetch = fetch,
): Promise<ProjectResponse[]> {
  return requestJson<ProjectResponse[]>('/api/projects', fetcher);
}

export async function createProject(
  request: SaveProjectRequest,
  fetcher: typeof fetch = fetch,
): Promise<ProjectResponse> {
  return requestJson<ProjectResponse>('/api/projects', fetcher, {
    body: JSON.stringify(request),
    headers: { 'content-type': 'application/json' },
    method: 'POST',
  });
}

export async function saveProject(
  projectId: string,
  request: SaveProjectRequest,
  fetcher: typeof fetch = fetch,
): Promise<ProjectResponse> {
  return requestJson<ProjectResponse>(
    `/api/projects/${encodeURIComponent(projectId)}`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function deleteProject(
  projectId: string,
  fetcher: typeof fetch = fetch,
): Promise<void> {
  await requestText(`/api/projects/${encodeURIComponent(projectId)}`, fetcher, {
    method: 'DELETE',
  });
}

export async function fetchPointSets(
  fetcher: typeof fetch = fetch,
): Promise<PointSetResponse[]> {
  const pointSets = await requestJson<PointSetResponse[]>('/api/point-sets', fetcher);
  return pointSets.map(normalizePointSet);
}

export async function createPointSet(
  request: SavePointSetRequest,
  fetcher: typeof fetch = fetch,
): Promise<PointSetResponse> {
  const pointSet = await requestJson<PointSetResponse>('/api/point-sets', fetcher, {
    body: JSON.stringify(pointSetRequestBody(request)),
    headers: { 'content-type': 'application/json' },
    method: 'POST',
  });
  return normalizePointSet(pointSet);
}

export async function savePointSet(
  pointSetId: string,
  request: SavePointSetRequest,
  fetcher: typeof fetch = fetch,
): Promise<PointSetResponse> {
  const pointSet = await requestJson<PointSetResponse>(
    `/api/point-sets/${encodeURIComponent(pointSetId)}`,
    fetcher,
    {
      body: JSON.stringify(pointSetRequestBody(request)),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
  return normalizePointSet(pointSet);
}

export async function deletePointSet(
  pointSetId: string,
  fetcher: typeof fetch = fetch,
): Promise<void> {
  await requestText(`/api/point-sets/${encodeURIComponent(pointSetId)}`, fetcher, {
    method: 'DELETE',
  });
}

function pointSetRequestBody(request: SavePointSetRequest): SavePointSetRequest {
  return {
    ...request,
    points: request.points.map((point) => ({
      ...point,
      valueType: pointSetValueTypeToCore(point.valueType),
    })),
  };
}

function normalizePointSet(pointSet: PointSetResponse): PointSetResponse {
  return {
    ...pointSet,
    points: (pointSet.points ?? []).map((point) => ({
      ...point,
      valueType: pointSetValueTypeFromCore(point.valueType),
    })),
  };
}

function pointSetValueTypeToCore(valueType: string): string {
  switch (valueType.toLowerCase()) {
    case 'bool':
    case 'boolean':
      return 'Boolean';
    case 'int32':
    case 'int64':
    case 'integer':
      return 'Integer';
    case 'string':
    case 'text':
      return 'Text';
    default:
      return 'Float';
  }
}

function pointSetValueTypeFromCore(valueType: string): string {
  switch (valueType.toLowerCase()) {
    case 'boolean':
      return 'bool';
    case 'integer':
      return 'int64';
    case 'text':
      return 'string';
    case 'float':
      return 'float32';
    default:
      return valueType;
  }
}

export async function fetchProducts(
  fetcher: typeof fetch = fetch,
): Promise<ProductResponse[]> {
  return requestJson<ProductResponse[]>('/api/products', fetcher);
}

export async function fetchProductVersions(
  productId: string,
  fetcher: typeof fetch = fetch,
): Promise<ProductVersionResponse[]> {
  return requestJson<ProductVersionResponse[]>(
    `/api/products/${encodeURIComponent(productId)}/versions`,
    fetcher,
  );
}

export async function createProduct(
  request: SaveProductRequest,
  fetcher: typeof fetch = fetch,
): Promise<ProductResponse> {
  return requestJson<ProductResponse>('/api/products', fetcher, {
    body: JSON.stringify(request),
    headers: { 'content-type': 'application/json' },
    method: 'POST',
  });
}

export async function saveProduct(
  productId: string,
  request: SaveProductRequest,
  fetcher: typeof fetch = fetch,
): Promise<ProductResponse> {
  return requestJson<ProductResponse>(
    `/api/products/${encodeURIComponent(productId)}`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function deleteProduct(
  productId: string,
  fetcher: typeof fetch = fetch,
): Promise<void> {
  await requestText(`/api/products/${encodeURIComponent(productId)}`, fetcher, {
    method: 'DELETE',
  });
}

export async function createProductVersion(
  productId: string,
  request: SaveProductVersionRequest,
  fetcher: typeof fetch = fetch,
): Promise<ProductVersionResponse> {
  return requestJson<ProductVersionResponse>(
    `/api/products/${encodeURIComponent(productId)}/versions`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'POST',
    },
  );
}

export async function saveProductVersion(
  productId: string,
  version: string,
  request: SaveProductVersionRequest,
  fetcher: typeof fetch = fetch,
): Promise<ProductVersionResponse> {
  return requestJson<ProductVersionResponse>(
    `/api/products/${encodeURIComponent(productId)}/versions/${encodeURIComponent(version)}`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function publishProductVersion(
  productId: string,
  version: string,
  fetcher: typeof fetch = fetch,
): Promise<ProductVersionResponse> {
  return requestJson<ProductVersionResponse>(
    `/api/products/${encodeURIComponent(productId)}/versions/${encodeURIComponent(version)}/publish`,
    fetcher,
    { method: 'POST' },
  );
}

export async function rollbackProductVersion(
  productId: string,
  version: string,
  fetcher: typeof fetch = fetch,
): Promise<ProductVersionResponse> {
  return requestJson<ProductVersionResponse>(
    `/api/products/${encodeURIComponent(productId)}/versions/${encodeURIComponent(version)}/rollback`,
    fetcher,
    { method: 'POST' },
  );
}

export async function deleteProductVersion(
  productId: string,
  version: string,
  fetcher: typeof fetch = fetch,
): Promise<void> {
  await requestText(
    `/api/products/${encodeURIComponent(productId)}/versions/${encodeURIComponent(version)}`,
    fetcher,
    { method: 'DELETE' },
  );
}

export async function fetchSummary(
  fetcher: typeof fetch = fetch,
): Promise<SummaryResponse> {
  return requestJson<SummaryResponse>('/api/summary', fetcher);
}

export async function fetchPointMappings(
  fetcher: typeof fetch = fetch,
): Promise<PointMappingResponse[]> {
  return requestJson<PointMappingResponse[]>('/api/point-mappings', fetcher);
}

export async function fetchEdgePointMappings(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<PointMappingResponse[]> {
  return requestJson<PointMappingResponse[]>(
    `/api/edges/${encodeURIComponent(edgeId)}/point-mappings`,
    fetcher,
  );
}

export async function fetchReleaseList(
  fetcher: typeof fetch = fetch,
): Promise<ReleaseListResponse> {
  return requestJson<ReleaseListResponse>('/api/releases', fetcher);
}

export async function fetchEdgeNodes(
  fetcher: typeof fetch = fetch,
): Promise<EdgeNodeResponse[]> {
  return requestJson<EdgeNodeResponse[]>('/api/edge-nodes', fetcher);
}

export async function createEdgeNode(
  request: CreateEdgeNodeRequest,
  fetcher: typeof fetch = fetch,
): Promise<EdgeNodeResponse> {
  return requestJson<EdgeNodeResponse>('/api/edge-nodes', fetcher, {
    body: JSON.stringify(request),
    headers: { 'content-type': 'application/json' },
    method: 'POST',
  });
}

export async function deleteEdgeNode(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<void> {
  await requestText(`/api/edge-nodes/${encodeURIComponent(edgeId)}`, fetcher, {
    method: 'DELETE',
  });
}

export async function bindEdgeProduct(
  edgeId: string,
  request: BindEdgeProductRequest,
  fetcher: typeof fetch = fetch,
): Promise<EdgeNodeResponse> {
  return requestJson<EdgeNodeResponse>(
    `/api/edge-nodes/${encodeURIComponent(edgeId)}/product-binding`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function generateEdgeAccessToken(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<EdgeAccessTokenResponse> {
  return requestJson<EdgeAccessTokenResponse>(
    `/api/edge-nodes/${encodeURIComponent(edgeId)}/access-token`,
    fetcher,
    { method: 'POST' },
  );
}

export async function fetchDeviceModels(
  fetcher: typeof fetch = fetch,
): Promise<DeviceModelResponse[]> {
  return requestJson<DeviceModelResponse[]>('/api/device-models', fetcher);
}

export async function createDeviceModelDraft(
  request: CreateDeviceModelRequest,
  fetcher: typeof fetch = fetch,
): Promise<DeviceModelResponse> {
  return requestJson<DeviceModelResponse>('/api/device-models', fetcher, {
    body: JSON.stringify(request),
    headers: { 'content-type': 'application/json' },
    method: 'POST',
  });
}

export async function saveDeviceModel(
  deviceType: string,
  request: SaveDeviceModelRequest,
  fetcher: typeof fetch = fetch,
): Promise<DeviceModelResponse> {
  return requestJson<DeviceModelResponse>(
    `/api/device-models/${encodeURIComponent(deviceType)}`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function deleteDeviceModel(
  deviceType: string,
  fetcher: typeof fetch = fetch,
): Promise<void> {
  await requestText(
    `/api/device-models/${encodeURIComponent(deviceType)}`,
    fetcher,
    {
      method: 'DELETE',
    },
  );
}

export async function fetchProtocolConnections(
  fetcher: typeof fetch = fetch,
): Promise<ProtocolConnectionResponse[]> {
  return requestJson<ProtocolConnectionResponse[]>(
    '/api/protocol-connections',
    fetcher,
  );
}

export async function fetchEdgeProtocolConnections(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<ProtocolConnectionResponse[]> {
  return requestJson<ProtocolConnectionResponse[]>(
    `/api/edges/${encodeURIComponent(edgeId)}/protocol-connections`,
    fetcher,
  );
}

export async function createPointMappingDraft(
  edgeId: string,
  request: CreatePointMappingRequest = {},
  fetcher: typeof fetch = fetch,
): Promise<PointMappingResponse> {
  return requestJson<PointMappingResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/point-mappings`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'POST',
    },
  );
}

export async function fetchCollectionTasks(
  fetcher: typeof fetch = fetch,
): Promise<CollectionTaskResponse[]> {
  return requestJson<CollectionTaskResponse[]>('/api/collection-tasks', fetcher);
}

export async function fetchEdgeCollectionTasks(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<CollectionTaskResponse[]> {
  return requestJson<CollectionTaskResponse[]>(
    `/api/edges/${encodeURIComponent(edgeId)}/collection-tasks`,
    fetcher,
  );
}

export async function fetchEdgeDataConfigs(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<DataConfigResponse[]> {
  return requestJson<DataConfigResponse[]>(
    `/api/edges/${encodeURIComponent(edgeId)}/data-configs`,
    fetcher,
  );
}

export async function createEdgeDataConfig(
  edgeId: string,
  request: SaveDataConfigRequest,
  fetcher: typeof fetch = fetch,
): Promise<DataConfigResponse> {
  return requestJson<DataConfigResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/data-configs`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'POST',
    },
  );
}

export async function createCollectionTaskDraft(
  edgeId: string,
  request: CreateCollectionTaskRequest,
  fetcher: typeof fetch = fetch,
): Promise<CollectionTaskResponse> {
  return requestJson<CollectionTaskResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/collection-tasks`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'POST',
    },
  );
}

export async function fetchAlgorithms(
  fetcher: typeof fetch = fetch,
): Promise<AlgorithmResponse[]> {
  return requestJson<AlgorithmResponse[]>('/api/algorithms', fetcher);
}

export async function fetchEdgeAlgorithms(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<AlgorithmResponse[]> {
  return requestJson<AlgorithmResponse[]>(
    `/api/edges/${encodeURIComponent(edgeId)}/algorithms`,
    fetcher,
  );
}

export async function createAlgorithmDraft(
  edgeId: string,
  request: CreateAlgorithmRequest,
  fetcher: typeof fetch = fetch,
): Promise<AlgorithmResponse> {
  return requestJson<AlgorithmResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/algorithms`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'POST',
    },
  );
}

export async function fetchAuditRecords(
  fetcher: typeof fetch = fetch,
): Promise<AuditRecordResponse[]> {
  return requestJson<AuditRecordResponse[]>('/api/audit-records', fetcher);
}

export async function fetchRuntimeStatus(
  fetcher: typeof fetch = fetch,
): Promise<RuntimeStatusResponse> {
  return requestJson<RuntimeStatusResponse>('/api/runtime-status', fetcher);
}

export async function fetchMqttUplink(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<MqttUplinkResponse> {
  return requestJson<MqttUplinkResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/mqtt-uplink`,
    fetcher,
  );
}

export async function saveMqttUplink(
  edgeId: string,
  request: SaveMqttUplinkRequest,
  fetcher: typeof fetch = fetch,
): Promise<MqttUplinkResponse> {
  return requestJson<MqttUplinkResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/mqtt-uplink`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function runDiscovery(
  edgeId: string,
  request: RunDiscoveryRequest,
  fetcher: typeof fetch = fetch,
): Promise<DiscoveryReportResponse> {
  return requestJson<DiscoveryReportResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/discovery/run`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'POST',
    },
  );
}

export async function fetchDiscoverySuggestions(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<PointMappingSuggestionResponse[]> {
  return requestJson<PointMappingSuggestionResponse[]>(
    `/api/edges/${encodeURIComponent(edgeId)}/discovery/suggestions`,
    fetcher,
  );
}

export async function savePointMapping(
  pointId: string,
  request: SavePointMappingRequest,
  fetcher: typeof fetch = fetch,
): Promise<PointMappingResponse> {
  return requestJson<PointMappingResponse>(
    `/api/point-mappings/${encodeURIComponent(pointId)}`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function saveEdgePointMapping(
  edgeId: string,
  pointId: string,
  request: SavePointMappingRequest,
  fetcher: typeof fetch = fetch,
): Promise<PointMappingResponse> {
  return requestJson<PointMappingResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/point-mappings/${encodeURIComponent(pointId)}`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function saveEdgeCollectionTask(
  edgeId: string,
  taskId: string,
  request: SaveCollectionTaskRequest,
  fetcher: typeof fetch = fetch,
): Promise<CollectionTaskResponse> {
  return requestJson<CollectionTaskResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/collection-tasks/${encodeURIComponent(taskId)}`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function deleteEdgeCollectionTask(
  edgeId: string,
  taskId: string,
  fetcher: typeof fetch = fetch,
): Promise<void> {
  await requestText(
    `/api/edges/${encodeURIComponent(edgeId)}/collection-tasks/${encodeURIComponent(taskId)}`,
    fetcher,
    {
      method: 'DELETE',
    },
  );
}

export async function saveEdgeDataConfig(
  edgeId: string,
  configId: string,
  request: SaveDataConfigRequest,
  fetcher: typeof fetch = fetch,
): Promise<DataConfigResponse> {
  return requestJson<DataConfigResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/data-configs/${encodeURIComponent(configId)}`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function deleteEdgeDataConfig(
  edgeId: string,
  configId: string,
  fetcher: typeof fetch = fetch,
): Promise<void> {
  await requestText(
    `/api/edges/${encodeURIComponent(edgeId)}/data-configs/${encodeURIComponent(configId)}`,
    fetcher,
    {
      method: 'DELETE',
    },
  );
}

export async function saveEdgeProtocolConnection(
  edgeId: string,
  connectionId: string,
  request: SaveProtocolConnectionRequest,
  fetcher: typeof fetch = fetch,
): Promise<ProtocolConnectionResponse> {
  return requestJson<ProtocolConnectionResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/protocol-connections/${encodeURIComponent(connectionId)}`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function deleteEdgeProtocolConnection(
  edgeId: string,
  connectionId: string,
  fetcher: typeof fetch = fetch,
): Promise<void> {
  await requestText(
    `/api/edges/${encodeURIComponent(edgeId)}/protocol-connections/${encodeURIComponent(connectionId)}`,
    fetcher,
    {
      method: 'DELETE',
    },
  );
}

export async function createEdgeProtocolConnection(
  edgeId: string,
  request: CreateProtocolConnectionRequest,
  fetcher: typeof fetch = fetch,
): Promise<ProtocolConnectionResponse> {
  return requestJson<ProtocolConnectionResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/protocol-connections`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'POST',
    },
  );
}

export async function saveEdgeAlgorithm(
  edgeId: string,
  algorithmId: string,
  request: SaveAlgorithmRequest,
  fetcher: typeof fetch = fetch,
): Promise<AlgorithmResponse> {
  return requestJson<AlgorithmResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/algorithms/${encodeURIComponent(algorithmId)}`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function deleteEdgeAlgorithm(
  edgeId: string,
  algorithmId: string,
  fetcher: typeof fetch = fetch,
): Promise<void> {
  await requestText(
    `/api/edges/${encodeURIComponent(edgeId)}/algorithms/${encodeURIComponent(algorithmId)}`,
    fetcher,
    {
      method: 'DELETE',
    },
  );
}

export async function deleteEdgePointMapping(
  edgeId: string,
  pointId: string,
  fetcher: typeof fetch = fetch,
): Promise<void> {
  await requestText(
    `/api/edges/${encodeURIComponent(edgeId)}/point-mappings/${encodeURIComponent(pointId)}`,
    fetcher,
    {
      method: 'DELETE',
    },
  );
}

export async function publishLatestRelease(
  edgeId = 'edge-dev',
  fetcher: typeof fetch = fetch,
): Promise<ReleaseListResponse> {
  return requestJson<ReleaseListResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/releases/publish`,
    fetcher,
    {
      method: 'POST',
    },
  );
}

export async function runConfigValidation(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<ManagementActionResponse> {
  return requestJson<ManagementActionResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/config/validate`,
    fetcher,
    {
      method: 'POST',
    },
  );
}

export async function runReleaseDiff(
  edgeId: string,
  fetcher: typeof fetch = fetch,
): Promise<ManagementActionResponse> {
  return requestJson<ManagementActionResponse>(
    `/api/edges/${encodeURIComponent(edgeId)}/releases/diff`,
    fetcher,
    {
      method: 'POST',
    },
  );
}

export async function runAgentSafetyCheck(
  fetcher: typeof fetch = fetch,
): Promise<AgentActionResponse> {
  return requestJson<AgentActionResponse>('/api/agent/safety-check', fetcher, {
    method: 'POST',
  });
}

export async function generateAgentSuggestions(
  fetcher: typeof fetch = fetch,
): Promise<AgentActionResponse> {
  return requestJson<AgentActionResponse>('/api/agent/suggestions', fetcher, {
    method: 'POST',
  });
}

export async function fetchAgentProviderStatus(
  fetcher: typeof fetch = fetch,
): Promise<AgentProviderStatusResponse> {
  return requestJson<AgentProviderStatusResponse>('/api/agent/provider', fetcher);
}

export async function sendAgentChat(
  request: AgentChatRequest,
  fetcher: typeof fetch = fetch,
): Promise<AgentChatResponse> {
  return requestJson<AgentChatResponse>('/api/agent/chat', fetcher, {
    body: JSON.stringify(request),
    headers: { 'content-type': 'application/json' },
    method: 'POST',
  });
}

export async function fetchAgentConversations(
  operatorId = 'console-operator',
  projectId?: string,
  fetcher: typeof fetch = fetch,
): Promise<AgentConversationResponse[]> {
  const params = new URLSearchParams({ operatorId });
  params.set('projectId', projectId ?? '');
  return requestJson<AgentConversationResponse[]>(
    `/api/agent/conversations?${params.toString()}`,
    fetcher,
  );
}

export async function deleteAgentConversation(
  conversationId: string,
  operatorId = 'console-operator',
  fetcher: typeof fetch = fetch,
): Promise<void> {
  await requestText(
    `/api/agent/conversations/${encodeURIComponent(conversationId)}?operatorId=${encodeURIComponent(operatorId)}`,
    fetcher,
    { method: 'DELETE' },
  );
}

export async function fetchAgentKnowledgeDocuments(
  projectId?: string,
  fetcher: typeof fetch = fetch,
): Promise<AgentKnowledgeDocumentResponse[]> {
  const query = projectId ? `?projectId=${encodeURIComponent(projectId)}` : '';
  return requestJson<AgentKnowledgeDocumentResponse[]>(
    `/api/agent/knowledge${query}`,
    fetcher,
  );
}

export async function createAgentKnowledgeDocument(
  request: SaveAgentKnowledgeDocumentRequest,
  fetcher: typeof fetch = fetch,
): Promise<AgentKnowledgeDocumentResponse> {
  return requestJson<AgentKnowledgeDocumentResponse>('/api/agent/knowledge', fetcher, {
    body: JSON.stringify(request),
    headers: { 'content-type': 'application/json' },
    method: 'POST',
  });
}

export async function saveAgentKnowledgeDocument(
  documentId: string,
  request: SaveAgentKnowledgeDocumentRequest,
  fetcher: typeof fetch = fetch,
): Promise<AgentKnowledgeDocumentResponse> {
  return requestJson<AgentKnowledgeDocumentResponse>(
    `/api/agent/knowledge/${encodeURIComponent(documentId)}`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'PUT',
    },
  );
}

export async function deleteAgentKnowledgeDocument(
  documentId: string,
  actor = 'console-operator',
  fetcher: typeof fetch = fetch,
): Promise<void> {
  await requestText(
    `/api/agent/knowledge/${encodeURIComponent(documentId)}?actor=${encodeURIComponent(actor)}`,
    fetcher,
    { method: 'DELETE' },
  );
}

export async function fetchAgentProposals(
  fetcher: typeof fetch = fetch,
): Promise<AgentProposalResponse[]> {
  return requestJson<AgentProposalResponse[]>('/api/agent/proposals', fetcher);
}

export async function createAgentProposal(
  request: CreateAgentProposalRequest,
  fetcher: typeof fetch = fetch,
): Promise<AgentProposalResponse> {
  return requestJson<AgentProposalResponse>('/api/agent/proposals', fetcher, {
    body: JSON.stringify(request),
    headers: { 'content-type': 'application/json' },
    method: 'POST',
  });
}

export async function reviewAgentProposal(
  proposalId: string,
  decision: 'approve' | 'reject',
  request: ReviewAgentProposalRequest,
  fetcher: typeof fetch = fetch,
): Promise<AgentProposalResponse> {
  return requestJson<AgentProposalResponse>(
    `/api/agent/proposals/${encodeURIComponent(proposalId)}/${decision}`,
    fetcher,
    {
      body: JSON.stringify(request),
      headers: { 'content-type': 'application/json' },
      method: 'POST',
    },
  );
}

async function requestJson<T>(
  path: string,
  fetcher: typeof fetch,
  init?: RequestInit,
): Promise<T> {
  const authenticatedInit = withApiToken(init);
  const response = authenticatedInit === undefined
    ? await fetcher(path)
    : await fetcher(path, authenticatedInit);
  if (!response.ok) {
    throw new Error(await responseErrorMessage(response, path));
  }

  return response.json() as Promise<T>;
}

async function requestText(
  path: string,
  fetcher: typeof fetch,
  init?: RequestInit,
): Promise<string> {
  const authenticatedInit = withApiToken(init);
  const response = authenticatedInit === undefined
    ? await fetcher(path)
    : await fetcher(path, authenticatedInit);
  if (!response.ok) {
    throw new Error(await responseErrorMessage(response, path));
  }

  return response.text();
}

function withApiToken(init?: RequestInit): RequestInit | undefined {
  const token = getApiToken();
  if (!token) return init;

  const headers = new Headers(init?.headers);
  headers.set('authorization', `Bearer ${token}`);
  return { ...init, headers };
}

async function responseErrorMessage(response: Response, path: string): Promise<string> {
  try {
    const payload = (await response.json()) as { error?: unknown; message?: unknown };
    if (typeof payload.message === 'string' && payload.message.trim()) {
      return payload.message;
    }
    if (typeof payload.error === 'string' && payload.error.trim()) {
      return payload.error;
    }
  } catch {
    // Fall through to text or status fallback.
  }

  try {
    const text = await response.text();
    if (text.trim()) {
      return text;
    }
  } catch {
    // Ignore body parsing errors and keep the deterministic fallback.
  }

  return `Failed to load ${path}: ${response.status}`;
}
