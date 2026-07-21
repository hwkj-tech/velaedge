export function displayError(error: unknown, fallback = '请稍后重试') {
  return error instanceof Error && error.message.trim() ? error.message : fallback;
}
