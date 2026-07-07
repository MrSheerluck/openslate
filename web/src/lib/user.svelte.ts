import { api } from "$lib/api";
import { setUnauthenticated } from "$lib/auth.svelte";

interface User {
  password: string;
  loading: boolean;
  errorMessage: string;
}

const DEFAULTS: User = {
  password: "",
  loading: false,
  errorMessage: ""
};

let user = $state<User>({ ...DEFAULTS });

export function getUser() {
  return user;
}

type LoginFlashMessage = { passwordChangeMessage: string | null };
export const loginFlashMessage: LoginFlashMessage = { passwordChangeMessage: null };

export async function setPassword(value: string) {
  user.password = value;
  user.loading = true;
  user.errorMessage = "";
  try {
    const res = await api("/api/auth/password", {
      method: "PUT",
      body: JSON.stringify({ password: value }),
    });
    if (res.ok) {
      loginFlashMessage.passwordChangeMessage = "Password change success. Please login again using new password.";
      // backend reclaimed tokens after password change, so force to log in again.
      setUnauthenticated();
    }
    else
      user.errorMessage = `Failed to change password: server status code ${res.status}`
  } catch (e) {
    user.errorMessage = `Failed to change password: network error ${e}`
  } finally {
    user.password = "";
    user.loading = false;
  }
}
