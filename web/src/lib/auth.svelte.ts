import { api } from "$lib/api";

let authenticated = $state<boolean | null>(null);

export function isAuthenticated(): boolean {
  return authenticated === true;
}

export function isLoading(): boolean {
  return authenticated === null;
}

export function setUnauthenticated() {
  authenticated = false;
}

export async function checkAuth(): Promise<void> {
  try {
    const res = await api("/api/auth/me");
    authenticated = res.ok;
  } catch {
    authenticated = false;
  }
}

async function hasUsers(): Promise<boolean> {
  try {
    const res = await api("/api/auth/status");
    if (res.ok) {
      const data = await res.json();
      return data.has_users === true;
    }
  } catch {}
  return true;
}

export async function login(password: string): Promise<boolean> {
  const exists = await hasUsers();
  const endpoint = exists ? "/api/auth/signin" : "/api/auth/signup";
  const res = await api(endpoint, {
    method: "POST",
    body: JSON.stringify({ password }),
  });
  if (res.ok) {
    authenticated = true;
    return true;
  }
  authenticated = false;
  return false;
}

export async function logout(): Promise<void> {
  await api("/api/auth/logout", { method: "POST" });
  authenticated = false;
}
