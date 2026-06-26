export interface SummaryResponse {
  edge_count: number;
  pending_release_count: number;
}

export interface PointMappingResponse {
  pointId: string;
  pointName: string;
  deviceId: string;
  deviceModel: string;
  semanticTelemetry: string;
  protocol: string;
  connection: string;
  address: string;
  valueType: string;
  readWrite: string;
  unit: string;
  scale: string;
  interval: string;
  range: string;
  qualityRule: string;
  status: string;
}

export interface SavePointMappingRequest {
  addressKind: string;
  addressValue: string;
  intervalMs: number;
  unit: string;
}

export interface ApplyResultResponse {
  edgeId: string;
  desiredVersion: string;
  reportedVersion: string;
  result: string;
  heartbeat: string;
}

export interface ReleaseListResponse {
  draftVersion: string;
  validationStatus: string;
  changeSummary: string;
  rolloutPolicy: string;
  applyResults: ApplyResultResponse[];
}
