<script lang="ts">
  import "../app.css";
  import * as auth from "$lib/auth.svelte";
  import * as theme from "$lib/theme.svelte";
  import * as prefs from "$lib/preferences.svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { api, interceptors } from "$lib/api";

  let { children } = $props();
  let currentPath = $derived(String(page.url.pathname));

  $effect(() => {
    auth.checkAuth();
    interceptors.response.use(function (r: Response) {
      if (r.status === 401) {
        auth.setUnauthenticated();
      }
      return r;
    });
  });

  $effect(() => {
    if (
      !auth.isLoading() &&
      !auth.isAuthenticated() &&
      currentPath !== "/login"
    ) {
      goto("/login");
    }
  });

  $effect(() => {
    if (auth.isAuthenticated()) {
      theme.loadFromServer();
      prefs.loadFromServer();
    }
  });
</script>

{#if currentPath === "/login"}
  {@render children()}
{:else if auth.isLoading()}
  <div class="flex min-h-screen items-center justify-center" style="background: var(--bg-page);">
    <p style="color: var(--text-secondary);">Loading...</p>
  </div>
{:else if auth.isAuthenticated()}
  {@render children()}
{:else}
  <div class="flex min-h-screen items-center justify-center" style="background: var(--bg-page);">
    <p style="color: var(--text-secondary);">Redirecting...</p>
  </div>
{/if}
