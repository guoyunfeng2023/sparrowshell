const base = import.meta.env.VITE_ADMIN_API_BASE || '/v1/admin';

export class ApiError extends Error {
  constructor(message, code, status) { super(message); this.code = code; this.status = status; }
}

export async function adminRequest(path, { token, onUnauthorized, ...options } = {}) {
  let response;
  try {
    response = await fetch(`${base}${path}`, {
      ...options,
      headers: {
        ...(options.body ? { 'Content-Type': 'application/json' } : {}),
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
        ...options.headers,
      },
    });
  } catch (error) {
    if (error.name === 'AbortError') throw error;
    throw new ApiError('无法连接管理服务，请确认 8070 服务已启动', 'network_unavailable', 0);
  }
  const body = await response.json().catch(() => ({}));
  if (!response.ok) {
    if (response.status === 401) onUnauthorized?.();
    throw new ApiError(body.error?.message || `请求失败（HTTP ${response.status}）`, body.error?.code || 'request_failed', response.status);
  }
  return body;
}

export async function adminDownload(path, { token, onUnauthorized } = {}) {
  let response;
  try {
    response = await fetch(`${base}${path}`, { headers: token ? { Authorization: `Bearer ${token}` } : {} });
  } catch {
    throw new ApiError('无法连接管理服务，请确认 8070 服务已启动', 'network_unavailable', 0);
  }
  if (!response.ok) {
    if (response.status === 401) onUnauthorized?.();
    const body = await response.json().catch(() => ({}));
    throw new ApiError(body.error?.message || `下载失败（HTTP ${response.status}）`, body.error?.code || 'download_failed', response.status);
  }
  const disposition = response.headers.get('Content-Disposition') || '';
  const encoded = disposition.match(/filename\*=UTF-8''([^;]+)/i)?.[1];
  const plain = disposition.match(/filename="?([^";]+)"?/i)?.[1];
  let filename = 'feedback-attachment';
  try { filename = encoded ? decodeURIComponent(encoded) : plain || filename; } catch { filename = plain || filename; }
  return { blob: await response.blob(), filename };
}

export const formatTime = value => value ? new Date(value).toLocaleString('zh-CN', { hour12: false }) : '—';
export const newOperationId = () => globalThis.crypto?.randomUUID?.() || `${Date.now()}-${Math.random().toString(16).slice(2)}`;
