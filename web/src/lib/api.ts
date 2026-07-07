const API_URL = import.meta.env.VITE_API_URL ?? "http://localhost:3001";

export type InterceptorResponse = (response: Response) => void;

class InterceptorManager {
  handlers: InterceptorResponse[];

  constructor() {
    this.handlers = [];
  }

  use(response: InterceptorResponse) {
    this.handlers.push(response);
    return this.handlers.length - 1;
  }

  clear() {
    if (this.handlers) {
      this.handlers = [];
    }
  }
}

export type Interceptors = { response: InterceptorManager };
export const interceptors: Interceptors = { response: new InterceptorManager() };

export async function api(path: string, init?: RequestInit): Promise<Response> {
  const headers: Record<string, string> = {
    ...(init?.headers as Record<string, string>),
  };
  if (!(init?.body instanceof FormData)) {
    headers["Content-Type"] = "application/json";
  }
  const res = await fetch(`${API_URL}${path}`, {
    ...init,
    credentials: "include",
    headers,
  });
  for (const handler of interceptors.response.handlers) {
    handler(res);
  }
  return res;
}

export async function uploadFile(
  path: string,
  file: File,
  extraFields?: Record<string, string>,
): Promise<Response> {
  const form = new FormData();
  form.append("file", file);
  if (extraFields) {
    for (const [key, val] of Object.entries(extraFields)) {
      form.append(key, val);
    }
  }
  return api(path, { method: "POST", body: form });
}

export async function uploadFiles(
  path: string,
  files: File[],
  extraFields?: Record<string, string>,
): Promise<Response> {
  const form = new FormData();
  for (const file of files) {
    form.append("file", file);
  }
  if (extraFields) {
    for (const [key, val] of Object.entries(extraFields)) {
      form.append(key, val);
    }
  }
  return api(path, { method: "POST", body: form });
}

export async function downloadFile(path: string, filename: string): Promise<void> {
  const res = await api(path);
  if (!res.ok) throw new Error(`Download failed: ${res.status}`);
  const blob = await res.blob();
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}
