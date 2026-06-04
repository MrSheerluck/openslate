<script lang="ts">
  import { onMount } from "svelte";
  import { api, uploadFile } from "$lib/api";

  type MediaItem = {
    id: string;
    filename: string;
    original_name: string;
    mime_type: string;
    size: number;
    note_id: string | null;
    tags: string[];
    created_at: string;
  };

  let items = $state<MediaItem[]>([]);
  let loading = $state(true);
  let searchQuery = $state("");
  let typeFilter = $state("");
  let selected = $state<MediaItem | null>(null);
  let detailItem = $state<MediaItem | null>(null);
  let uploadProgress = $state(false);
  let dragOver = $state(false);
  let ctxMenu = $state<{ x: number; y: number; item: MediaItem } | null>(null);

  onMount(() => loadMedia());

  async function loadMedia() {
    loading = true;
    try {
      const params = new URLSearchParams();
      if (searchQuery) params.set("q", searchQuery);
      if (typeFilter) params.set("type", typeFilter);
      const res = await api(`/api/media?${params}`);
      if (res.ok) items = await res.json();
      else items = [];
    } catch {
      items = [];
    }
    loading = false;
  }

  function handleSearchInput() {
    loadMedia();
  }

  async function handleUpload(file: File) {
    uploadProgress = true;
    try {
      const res = await uploadFile("/api/media", file);
      if (res.ok) loadMedia();
    } catch {
      // ignore
    }
    uploadProgress = false;
  }

  function onFilePick(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (file) handleUpload(file);
    input.value = "";
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    dragOver = false;
    const file = e.dataTransfer?.files?.[0];
    if (file) handleUpload(file);
  }

  function formatSize(bytes: number) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function formatDate(iso: string) {
    return iso.slice(0, 10);
  }

  function isImage(mime: string) {
    return mime.startsWith("image/");
  }

  function mediaUrl(item: MediaItem) {
    return `${import.meta.env.VITE_API_URL || "http://localhost:3001"}/api/media/${item.id}/file`;
  }

  async function deleteItem(item: MediaItem) {
    if (!confirm(`Delete "${item.original_name}"?`)) return;
    await api(`/api/media/${item.id}`, { method: "DELETE" });
    if (detailItem?.id === item.id) detailItem = null;
    loadMedia();
  }

  function handleCtxMenu(e: MouseEvent, item: MediaItem) {
    e.preventDefault();
    ctxMenu = { x: e.clientX, y: e.clientY, item };
  }

  function closeCtxMenu() {
    ctxMenu = null;
  }

  function openDetail(item: MediaItem) {
    detailItem = item;
  }

  function closeDetail() {
    detailItem = null;
  }

  async function copyCurrentLink() {
    if (!detailItem) return;
    await copyLink(detailItem);
  }

  async function deleteCurrentItem() {
    if (!detailItem) return;
    await deleteItem(detailItem);
  }

  async function copyLink(item: MediaItem) {
    const url = mediaUrl(item);
    try {
      await navigator.clipboard.writeText(url);
    } catch {
      // fallback
    }
    closeCtxMenu();
  }

  function fileTypeIcon(mime: string) {
    if (mime.startsWith("image/")) return "🖼";
    if (mime.startsWith("video/")) return "🎬";
    if (mime === "application/pdf") return "📄";
    return "📎";
  }

  function mimeCategory(mime: string): string {
    if (mime.startsWith("image/")) return "image";
    if (mime.startsWith("video/")) return "video";
    return "document";
  }

  function getThumbnail(item: MediaItem): string | null {
    if (isImage(item.mime_type)) return mediaUrl(item);
    return null;
  }

  async function addTag(tag: string) {
    if (!detailItem) return;
    const newTags = [...detailItem.tags, tag];
    const res = await api(`/api/media/${detailItem.id}`, {
      method: "PUT",
      body: JSON.stringify({ tags: newTags }),
    });
    if (res.ok) {
      detailItem = await res.json();
      loadMedia();
    }
  }

  async function removeTag(tag: string) {
    if (!detailItem) return;
    const newTags = detailItem.tags.filter((t) => t !== tag);
    const res = await api(`/api/media/${detailItem.id}`, {
      method: "PUT",
      body: JSON.stringify({ tags: newTags }),
    });
    if (res.ok) {
      detailItem = await res.json();
      loadMedia();
    }
  }

  async function addTagInput(e: KeyboardEvent) {
    if (e.key === "Enter") {
      const input = e.target as HTMLInputElement;
      const val = input.value.trim();
      if (val) {
        input.value = "";
        await addTag(val);
      }
    }
  }
</script>

<!-- Detail panel overlay -->
{#if detailItem}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-30" role="none" onclick={closeDetail}></div>
  <div
    class="fixed right-0 top-0 h-full w-80 z-40 shadow-2xl flex flex-col overflow-y-auto"
    style="background: var(--bg-sidebar); border-left: 1px solid var(--border-color);"
  >
    <div class="p-3 border-b flex items-center justify-between" style="border-color: var(--border-color);">
      <span class="font-semibold text-sm" style="color: var(--text-primary);">Media details</span>
      <button onclick={closeDetail} class="text-xs" style="color: var(--text-tertiary);">Close</button>
    </div>
    <div class="p-3 flex-1 space-y-3">
      {#if isImage(detailItem.mime_type)}
        <img
          src={mediaUrl(detailItem)}
          alt={detailItem.original_name}
          class="w-full rounded border"
          style="border-color: var(--border-color);"
        />
      {:else}
        <div
          class="w-full h-40 rounded border flex items-center justify-center text-4xl"
          style="border-color: var(--border-color); background: var(--bg-editor);"
        >
          {fileTypeIcon(detailItem.mime_type)}
        </div>
      {/if}

      <div>
        <p class="text-xs" style="color: var(--text-tertiary);">Name</p>
        <p class="text-sm truncate" style="color: var(--text-primary);">{detailItem.original_name}</p>
      </div>
      <div>
        <p class="text-xs" style="color: var(--text-tertiary);">Type</p>
        <p class="text-sm" style="color: var(--text-primary);">{detailItem.mime_type}</p>
      </div>
      <div>
        <p class="text-xs" style="color: var(--text-tertiary);">Size</p>
        <p class="text-sm" style="color: var(--text-primary);">{formatSize(detailItem.size)}</p>
      </div>
      <div>
        <p class="text-xs" style="color: var(--text-tertiary);">Uploaded</p>
        <p class="text-sm" style="color: var(--text-primary);">{formatDate(detailItem.created_at)}</p>
      </div>

      <div>
        <p class="text-xs mb-1" style="color: var(--text-tertiary);">Tags</p>
        <div class="flex flex-wrap gap-1 mb-1">
          {#each detailItem.tags as tag}
            <span
              class="text-xs px-1.5 py-0.5 rounded flex items-center gap-1 bg-tag"
              style="color: var(--text-secondary);"
            >
              {tag}
              <button onclick={() => removeTag(tag)} class="text-xs leading-none" style="color: var(--text-danger);">&times;</button>
            </span>
          {/each}
        </div>
        <input
          placeholder="Add tag..."
          onkeydown={addTagInput}
          class="w-full text-xs px-1.5 py-1 rounded outline-none"
          style="color: var(--text-primary); background: var(--bg-editor); border: 1px solid var(--border-input);"
        />
      </div>

      <div class="pt-2 flex gap-2">
        <a
          href={mediaUrl(detailItem)}
          target="_blank"
          rel="noreferrer"
          class="text-xs px-3 py-1.5 rounded"
          style="color: var(--text-btn-primary); background: var(--bg-btn-primary); text-decoration: none;"
        >
          Open
        </a>
        <button
          onclick={copyCurrentLink}
          class="text-xs px-3 py-1.5 rounded"
          style="color: var(--text-primary); background: var(--bg-editor); border: 1px solid var(--border-input);"
        >
          Copy link
        </button>
        <button
          onclick={deleteCurrentItem}
          class="text-xs px-3 py-1.5 rounded"
          style="color: var(--text-btn-primary); background: var(--text-danger); border: none;"
        >
          Delete
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Context menu -->
{#if ctxMenu}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-40" role="none" onclick={closeCtxMenu}></div>
  <div
    class="fixed z-50 rounded border shadow-lg py-1 min-w-[140px]"
    style="left: {ctxMenu.x}px; top: {ctxMenu.y}px; background: var(--bg-sidebar); border-color: var(--border-color);"
  >
    <button onclick={() => { openDetail(ctxMenu!.item); closeCtxMenu(); }} class="ctx-menu-item">
      View details
    </button>
    <button onclick={() => copyLink(ctxMenu!.item)} class="ctx-menu-item">
      Copy link
    </button>
    <div class="border-t" style="border-color: var(--border-color);"></div>
    <button
      onclick={() => { deleteItem(ctxMenu!.item); closeCtxMenu(); }}
      class="ctx-menu-item"
      style="color: var(--text-danger);"
    >
      Delete
    </button>
  </div>
{/if}

<!-- Main gallery -->
<div
  role="region"
  aria-label="Media gallery"
  class="flex flex-col h-full"
  ondragover={(e) => { e.preventDefault(); dragOver = true; }}
  ondragleave={() => { dragOver = false; }}
  ondrop={onDrop}
  oncontextmenu={(e) => e.preventDefault()}
>
  <!-- Search + filter + upload bar -->
  <div class="p-3 border-b space-y-2" style="border-color: var(--border-color);">
    <div class="flex gap-2 items-center">
      <input
        bind:value={searchQuery}
        oninput={handleSearchInput}
        placeholder="Search media..."
        class="flex-1 text-sm px-2 py-1.5 rounded outline-none"
        style="color: var(--text-primary); background: var(--bg-editor); border: 1px solid var(--border-input);"
      />
      <label
        class="text-xs px-3 py-1.5 rounded cursor-pointer"
        style="color: var(--text-btn-primary); background: var(--bg-btn-primary);"
      >
        {uploadProgress ? "Uploading..." : "Upload"}
        <input type="file" onchange={onFilePick} class="hidden" />
      </label>
    </div>
    <div class="flex gap-1.5 flex-wrap">
      {#each ["", "image", "video", "application/pdf"] as t}
        <button
          onclick={() => { typeFilter = t; loadMedia(); }}
          class="text-xs px-2 py-1 rounded"
          class:active={typeFilter === t}
          style="color: var(--text-secondary); background: var(--bg-tag); border: 1px solid transparent;"
        >
          {t || "All"}
        </button>
      {/each}
    </div>
  </div>

  <!-- Grid -->
  <div class="flex-1 overflow-y-auto p-3">
    {#if loading}
      <p class="text-sm" style="color: var(--text-tertiary);">Loading...</p>
    {:else if items.length === 0}
      <div class="flex flex-col items-center justify-center h-full gap-2">
        <p class="text-sm" style="color: var(--text-tertiary);">
          {#if dragOver}
            Drop file to upload
          {:else}
            No media yet
          {/if}
        </p>
        {#if !dragOver}
          <p class="text-xs" style="color: var(--text-tertiary);">Drop files here or use the upload button</p>
        {/if}
      </div>
    {:else}
      <div class="grid grid-cols-3 gap-2">
        {#each items as item}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            role="none"
            class="relative group rounded border overflow-hidden cursor-pointer"
            style="border-color: var(--border-color); aspect-ratio: 1;"
            onclick={() => openDetail(item)}
            oncontextmenu={(e) => handleCtxMenu(e, item)}
          >
            {#if getThumbnail(item)}
              <img
                src={getThumbnail(item)}
                alt={item.original_name}
                class="w-full h-full object-cover"
              />
            {:else}
              <div class="w-full h-full flex flex-col items-center justify-center gap-1" style="background: var(--bg-editor);">
                <span class="text-2xl">{fileTypeIcon(item.mime_type)}</span>
                <span class="text-xs px-1 truncate max-w-full" style="color: var(--text-secondary);">{item.original_name}</span>
              </div>
            {/if}
            <div
              class="absolute bottom-0 left-0 right-0 px-1.5 py-1 text-xs truncate opacity-0 group-hover:opacity-100 transition-opacity"
              style="background: rgba(0,0,0,0.6); color: white;"
            >
              {item.original_name}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

{#if dragOver}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    role="none"
    class="fixed inset-0 z-50 flex items-center justify-center"
    style="background: rgba(0,0,0,0.3);"
  >
    <div class="rounded-xl p-8 text-center" style="background: var(--bg-sidebar); border: 3px dashed var(--border-input);">
      <p class="text-lg font-semibold" style="color: var(--text-primary);">Drop file to upload</p>
    </div>
  </div>
{/if}

<style>
  .ctx-menu-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 0.375rem 0.75rem;
    font-size: 0.8125rem;
    border: none;
    background: transparent;
    cursor: pointer;
    color: var(--text-primary);
  }
  .ctx-menu-item:hover {
    background: var(--bg-note-hover);
  }
  button.active {
    background: var(--bg-btn-primary) !important;
    color: var(--text-btn-primary) !important;
  }
</style>
