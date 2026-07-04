import { api } from "$lib/api";
import { setUnauthenticated } from "$lib/auth.svelte";

interface User {
  password: string;
}

const DEFAULTS: User = {
  password: "",
};

let user = $state<User>({ ...DEFAULTS });

export function getUser(): User {
  return user;
}

export async function setPassword(value: string) {
  user.password = value;
  try {
    const res = await api("/api/auth/password", {
      method: "PUT",
      body: JSON.stringify({ password: value }),
    });
    if (res.ok)
      // backend reclaimed tokens after password change, so force to log in again.
      setUnauthenticated();
    else
      console.error("Failed to change user password:", res.status);
  } catch (e) {
    console.error("Failed to change user password:", e);
  }
}
