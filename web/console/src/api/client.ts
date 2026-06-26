import type {
  PointMappingResponse,
  ReleaseListResponse,
  SummaryResponse,
} from './types';

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

export async function fetchReleaseList(
  fetcher: typeof fetch = fetch,
): Promise<ReleaseListResponse> {
  return requestJson<ReleaseListResponse>('/api/releases', fetcher);
}

async function requestJson<T>(
  path: string,
  fetcher: typeof fetch,
): Promise<T> {
  const response = await fetcher(path);
  if (!response.ok) {
    throw new Error(`Failed to load ${path}: ${response.status}`);
  }

  return response.json() as Promise<T>;
}
