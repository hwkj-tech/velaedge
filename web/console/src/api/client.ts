import type { SummaryResponse } from './types';

export async function fetchSummary(
  fetcher: typeof fetch = fetch,
): Promise<SummaryResponse> {
  const response = await fetcher('/api/summary');

  if (!response.ok) {
    throw new Error(`Failed to load summary: ${response.status}`);
  }

  return response.json() as Promise<SummaryResponse>;
}
