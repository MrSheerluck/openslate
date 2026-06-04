<script lang="ts">
  import type { Editor } from "@tiptap/core";
  import { api } from "$lib/api";
  import { common } from "lowlight";

  let {
    editor,
    noteId = "",
    onUploadClick,
    onOpenMediaPicker,
    onImportComplete,
    uploading = false,
  }: {
    editor: Editor;
    noteId?: string;
    onUploadClick?: () => void;
    onOpenMediaPicker?: () => void;
    onImportComplete?: () => void;
    uploading?: boolean;
  } = $props();

  let availableLangs = $state<string[]>(Object.keys(common).sort());
  let currentLang = $state("");
  let isInCodeBlock = $state(false);
  let isInTable = $state(false);
  let showMediaMenu = $state(false);
  let showUrlInput = $state(false);
  let urlInput = $state("");
  let importingUrl = $state(false);

  $effect(() => {
    if (!editor) return;
    const sync = () => {
      isInCodeBlock = editor.isActive("codeBlock");
      if (isInCodeBlock) {
        const attrs = editor.getAttributes("codeBlock");
        currentLang = attrs.language || "";
      } else {
        currentLang = "";
      }
      isInTable = editor.isActive("table");
    };
    sync();
    editor.on("selectionUpdate", sync);
    editor.on("transaction", sync);
    return () => {
      editor.off("selectionUpdate", sync);
      editor.off("transaction", sync);
    };
  });

  function setCodeLanguage(lang: string) {
    editor.chain().focus().updateAttributes("codeBlock", { language: lang }).run();
  }

  function unsetCodeLanguage() {
    editor.chain().focus().updateAttributes("codeBlock", { language: null }).run();
  }

  function addLink() {
    const url = prompt("Enter URL:");
    if (url) {
      editor.chain().focus().setLink({ href: url }).run();
    }
  }

  async function submitUrlImport() {
    const url = urlInput.trim();
    if (!url) return;
    importingUrl = true;
    try {
      const body: Record<string, string> = { url };
      if (noteId) body.note_id = noteId;
      const res = await api(`/api/media/from-url?t=${Date.now()}`, {
        method: "POST",
        body: JSON.stringify(body),
      });
      if (res.ok) {
        const data = await res.json();
        const mediaUrl = `${import.meta.env.VITE_API_URL || "http://localhost:3001"}/api/media/${data.id}/file`;
        editor.chain().focus().setImage({ src: mediaUrl }).run();
        onImportComplete?.();
        showMediaMenu = false;
        showUrlInput = false;
        urlInput = "";
      } else {
        let msg = `Import failed (${res.status})`;
        try { const j = await res.json(); if (j.error) msg += `: ${j.error}`; } catch {}
        alert(msg);
      }
    } catch (e) {
      alert(`Import error: ${e instanceof Error ? e.message : e}`);
    }
    importingUrl = false;
  }
</script>

<div class="toolbar flex flex-wrap items-center gap-0.5 border-b px-2 py-1.5 bg-toolbar sticky top-0 z-10" style="border-color: var(--border-color);">
  <!-- Heading -->
  <button
    onclick={() => editor.chain().focus().toggleHeading({ level: 1 }).run()}
    class="toolbar-btn font-bold"
    class:is-active={editor.isActive("heading", { level: 1 })}
    title="Heading 1"
  >H1</button>
  <button
    onclick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}
    class="toolbar-btn font-bold"
    class:is-active={editor.isActive("heading", { level: 2 })}
    title="Heading 2"
  >H2</button>
  <button
    onclick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()}
    class="toolbar-btn font-bold"
    class:is-active={editor.isActive("heading", { level: 3 })}
    title="Heading 3"
  >H3</button>

  <span class="w-px h-5 mx-1" style="background: var(--border-color);"></span>

  <!-- Inline formatting -->
  <button
    onclick={() => editor.chain().focus().toggleBold().run()}
    class="toolbar-btn"
    class:is-active={editor.isActive("bold")}
    title="Bold"
  ><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4h8a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z"/><path d="M6 12h9a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z"/></svg></button>
  <button
    onclick={() => editor.chain().focus().toggleItalic().run()}
    class="toolbar-btn"
    class:is-active={editor.isActive("italic")}
    title="Italic"
  ><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M19 4h-9"/><path d="M14 20H5"/><path d="M15 4 9 20"/></svg></button>
  <button
    onclick={() => editor.chain().focus().toggleStrike().run()}
    class="toolbar-btn"
    class:is-active={editor.isActive("strike")}
    title="Strikethrough"
  ><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M16 4H9a3 3 0 0 0-2.83 4"/><path d="M14 12a4 4 0 0 1 0 8H6"/><path d="M4 12h16"/></svg></button>
  <button
    onclick={() => editor.chain().focus().toggleUnderline().run()}
    class="toolbar-btn"
    class:is-active={editor.isActive("underline")}
    title="Underline"
  ><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 3v7a6 6 0 0 0 12 0V3"/><path d="M4 21h16"/></svg></button>
  <button
    onclick={() => editor.chain().focus().toggleCode().run()}
    class="toolbar-btn"
    class:is-active={editor.isActive("code")}
    title="Inline code"
  ><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m10 20-8-8 8-8"/><path d="m14 4 8 8-8 8"/></svg></button>
  <button
    onclick={() => editor.chain().focus().toggleHighlight().run()}
    class="toolbar-btn"
    class:is-active={editor.isActive("highlight")}
    title="Highlight"
  ><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m12 3-6 8h4l-3 6 9-8h-5z"/></svg></button>

  <span class="w-px h-5 mx-1" style="background: var(--border-color);"></span>

  <!-- Lists -->
  <button
    onclick={() => editor.chain().focus().toggleBulletList().run()}
    class="toolbar-btn"
    class:is-active={editor.isActive("bulletList")}
    title="Bullet list"
  ><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="9" y1="6" x2="20" y2="6"/><line x1="9" y1="12" x2="20" y2="12"/><line x1="9" y1="18" x2="20" y2="18"/><circle cx="5" cy="6" r="1"/><circle cx="5" cy="12" r="1"/><circle cx="5" cy="18" r="1"/></svg></button>
  <button
    onclick={() => editor.chain().focus().toggleOrderedList().run()}
    class="toolbar-btn"
    class:is-active={editor.isActive("orderedList")}
    title="Ordered list"
  ><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="11" y1="5" x2="21" y2="5"/><line x1="11" y1="12" x2="21" y2="12"/><line x1="11" y1="19" x2="21" y2="19"/><path d="M4 4h1v5"/><path d="M4 9h2"/><path d="M5 11v3l2-2"/><path d="M6.5 20H3.4c0-1 2.6-1.9 2.6-3.5a1.5 1.5 0 0 0-2.6-1"/></svg></button>
  <button
    onclick={() => editor.chain().focus().toggleTaskList().run()}
    class="toolbar-btn"
    class:is-active={editor.isActive("taskList")}
    title="Task list"
  ><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><path d="m9 12 2 2 4-4"/></svg></button>

  <span class="w-px h-5 mx-1" style="background: var(--border-color);"></span>

  <!-- Block elements -->
  <button
    onclick={() => editor.chain().focus().toggleBlockquote().run()}
    class="toolbar-btn"
    class:is-active={editor.isActive("blockquote")}
    title="Blockquote"
  ><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 21c3 0 7-1 7-8V5c0-1.25-.756-2.017-2-2H4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2 1 0 1 0 1 1v1c0 1-1 2-2 2s-1 .008-1 1.031V20c0 1 0 1 1 1z"/><path d="M15 21c3 0 7-1 7-8V5c0-1.25-.757-2.017-2-2h-4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2h.75c0 2.25.25 4-2.75 4v3c0 1 0 1 1 1z"/></svg></button>
  <button
    onclick={() => editor.chain().focus().toggleCodeBlock().run()}
    class="toolbar-btn"
    class:is-active={isInCodeBlock}
    title="Code block"
  ><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m8 3-7 9 7 9"/><path d="m16 3 7 9-7 9"/></svg></button>
  <button onclick={addLink} class="toolbar-btn" class:is-active={editor.isActive("link")} title="Link">
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>
  </button>
  <button
    onclick={() => editor.chain().focus().setHorizontalRule().run()}
    class="toolbar-btn"
    title="Horizontal rule"
  ><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12h18"/></svg></button>

  <span class="w-px h-5 mx-1" style="background: var(--border-color);"></span>

  <button
    onclick={() => editor.chain().focus().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run()}
    class="toolbar-btn"
    class:is-active={isInTable}
    title="Insert table"
  ><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="3" y1="9" x2="21" y2="9"/><line x1="3" y1="15" x2="21" y2="15"/><line x1="9" y1="3" x2="9" y2="21"/><line x1="15" y1="3" x2="15" y2="21"/></svg></button>

  {#if isInTable}
    <span class="w-px h-5 mx-1" style="background: var(--border-color);"></span>

    <button onclick={() => editor.chain().focus().addRowBefore().run()} class="toolbar-btn" title="Insert row above">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="3" y1="12" x2="21" y2="12"/><line x1="12" y1="3" x2="12" y2="8"/><rect x="4" y="14" width="16" height="7" rx="1"/></svg>
    </button>
    <button onclick={() => editor.chain().focus().addRowAfter().run()} class="toolbar-btn" title="Insert row below">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="3" y1="12" x2="21" y2="12"/><line x1="12" y1="16" x2="12" y2="21"/><rect x="4" y="3" width="16" height="7" rx="1"/></svg>
    </button>
    <button onclick={() => editor.chain().focus().addColumnBefore().run()} class="toolbar-btn" title="Insert column left">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="3" x2="12" y2="21"/><line x1="3" y1="12" x2="8" y2="12"/><rect x="10" y="4" width="11" height="16" rx="1"/></svg>
    </button>
    <button onclick={() => editor.chain().focus().addColumnAfter().run()} class="toolbar-btn" title="Insert column right">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="3" x2="12" y2="21"/><line x1="16" y1="12" x2="21" y2="12"/><rect x="3" y="4" width="11" height="16" rx="1"/></svg>
    </button>

    <span class="w-px h-5 mx-1" style="background: var(--border-color);"></span>

    <button onclick={() => editor.chain().focus().deleteRow().run()} class="toolbar-btn" title="Delete row">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/><line x1="10" y1="11" x2="10" y2="17"/><line x1="14" y1="11" x2="14" y2="17"/></svg>
    </button>
    <button onclick={() => editor.chain().focus().deleteColumn().run()} class="toolbar-btn" title="Delete column">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M6 3v18"/><rect x="3" y="6" width="7" height="14" rx="1"/><rect x="14" y="6" width="7" height="14" rx="1"/></svg>
    </button>
    <button onclick={() => editor.chain().focus().deleteTable().run()} class="toolbar-btn" title="Delete table" style="color: var(--text-danger);">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="3" y1="9" x2="21" y2="9"/><line x1="3" y1="15" x2="21" y2="15"/><line x1="9" y1="3" x2="9" y2="21"/><line x1="15" y1="3" x2="15" y2="21"/><line x1="3" y1="3" x2="21" y2="21"/></svg>
    </button>
  {/if}

  <span class="w-px h-5 mx-1" style="background: var(--border-color);"></span>

  <div class="relative">
    <button
      onclick={() => showMediaMenu = !showMediaMenu}
      class="toolbar-btn"
      class:is-active={showMediaMenu}
      title="Add media"
    >
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>
    </button>
    {#if showMediaMenu}
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div role="none" class="fixed inset-0 z-40" onclick={() => showMediaMenu = false}></div>
      <div
        class="absolute left-0 top-full mt-1 z-50 rounded border shadow-lg py-1 min-w-[160px]"
        style="background: var(--bg-sidebar); border-color: var(--border-color);"
      >
        {#if showUrlInput}
          <div class="flex flex-col gap-1 p-2" style="min-width: 240px;">
            <input
              bind:value={urlInput}
              placeholder="Paste image/video URL..."
              onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); submitUrlImport(); } }}
              class="w-full text-xs px-2 py-1 rounded outline-none"
              style="color: var(--text-primary); background: var(--bg-editor); border: 1px solid var(--border-input);"
            />
            <div class="flex gap-1 justify-end">
              <button
                onclick={() => { showUrlInput = false; urlInput = ""; }}
                class="text-xs px-2 py-1 rounded"
                style="color: var(--text-secondary); background: var(--bg-editor); border: 1px solid var(--border-input); cursor: pointer;"
              >Cancel</button>
              <button
                onclick={submitUrlImport}
                class="text-xs px-2 py-1 rounded"
                style="color: var(--text-btn-primary); background: var(--bg-btn-primary); cursor: pointer; border: none;"
              >{importingUrl ? "Importing..." : "Import"}</button>
            </div>
          </div>
        {:else}
          <button onclick={() => { showMediaMenu = false; onUploadClick?.(); }} class="media-menu-item">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
            Upload from file
          </button>
          <button onclick={() => { showMediaMenu = false; onOpenMediaPicker?.(); }} class="media-menu-item">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>
            Browse media
          </button>
          <button onclick={() => { showUrlInput = true; }} class="media-menu-item">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>
            Import from URL
          </button>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Code block language selector -->
  {#if isInCodeBlock}
    <span class="w-px h-5 mx-1" style="background: var(--border-color);"></span>
    <select
      class="text-xs border rounded px-1 py-0.5 max-w-[120px] bg-editor"
      style="border-color: var(--border-input); color: var(--text-primary);"
      value={currentLang}
      onchange={(e) => {
        const val = (e.target as HTMLSelectElement).value;
        if (val === "") unsetCodeLanguage();
        else setCodeLanguage(val);
      }}
    >
      <option value="">No language</option>
      {#each availableLangs as lang}
        <option value={lang} selected={currentLang === lang}>{lang}</option>
      {/each}
    </select>
  {/if}
</div>

<style>
  .toolbar {
    font-size: 13px;
  }
  .toolbar-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 26px;
    border-radius: 4px;
    border: none;
    background: transparent;
    cursor: pointer;
    color: var(--toolbar-btn-text);
    font-size: 13px;
    line-height: 1;
    outline: none;
  }
  .toolbar-btn:hover {
    background: var(--toolbar-btn-hover-bg);
  }
  .toolbar-btn.is-active {
    background: var(--toolbar-btn-active-bg);
    color: var(--toolbar-btn-active-text);
  }
  .media-menu-item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    text-align: left;
    padding: 0.375rem 0.75rem;
    font-size: 0.8125rem;
    border: none;
    background: transparent;
    cursor: pointer;
    color: var(--text-primary);
  }
  .media-menu-item:hover {
    background: var(--bg-note-hover);
  }
</style>
