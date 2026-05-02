export class ApiClient {
  constructor(private readonly token: string | null) {}

  async request<T = unknown>(path: string, options: RequestInit = {}): Promise<T> {
    const separator = path.includes('?') ? '&' : '?';
    const response = await fetch(`${path}${separator}token=${encodeURIComponent(this.token || '')}`, {
      headers: { 'content-type': 'application/json', ...(options.headers || {}) },
      ...options,
    });
    if (!response.ok) {
      let message = await response.text();
      try { message = JSON.parse(message).error || message; } catch (_) {}
      throw new Error(message);
    }
    return response.json();
  }

  post<T = unknown>(path: string, body?: unknown): Promise<T> {
    return this.request<T>(path, {
      method: 'POST',
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
  }
}

export function withStateVersion(path: string, version: number | null | undefined): string {
  const separator = path.includes('?') ? '&' : '?';
  return `${path}${separator}state_version=${encodeURIComponent(version ?? '')}`;
}
