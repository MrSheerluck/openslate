# Architecture

## Overview

OpenSlate is a single-user, self-hosted note-taking app with a SvelteKit frontend and a Rust/Axum backend. Notes are stored as rich text (Tiptap's JSON format) in SQLite, with full-text search via FTS5. Media files are stored on Cloudflare R2.

The database layer is abstracted behind a `Database` enum  --  the same application code supports both local SQLite (via `sqlx`) and Turso distributed SQLite (via `libsql`). Switching is a one-line config change and a compile flag. See the [Setup Guide](setup.md) for details.

```
[Browser] ──► [SvelteKit SPA] ──► [Axum API :3001] ──► [SQLite]
                      │                    │
                      │            [Cloudflare R2] (media files)
                      │
              [Tiptap Editor]
```

- The frontend is a **single-page application**  --  all note editing, search, and media management happens on one page (`/`).
- The backend exposes a **REST API** on port 3001.
- Authentication uses a **JWT stored in an httpOnly cookie** (30-day expiry).
- The frontend proxies API requests during development via Vite's env config.

## Project Structure

```
api/src/
├── main.rs          # Entry point, router, CORS, app state
├── config.rs        # Environment variable loading
├── db/
│   ├── mod.rs       # Database enum, DbParam, Row, FromRow trait, migrations
│   ├── sqlx_db.rs   # Local SQLite backend (via sqlx)
│   └── libsql_db.rs # Turso/libsql backend (via libsql crate)
├── auth.rs          # Login/logout, JWT middleware, /api/auth/me
├── notes.rs         # CRUD for notes, link parsing, backlinks
├── search.rs        # Full-text search via FTS5
├── media.rs         # Media upload/list/serve/delete, R2 integration
└── preferences.rs   # Key-value preferences store (theme sync)

web/src/
├── lib/
│   ├── api.ts               # fetch() wrapper, auto-JSON, credentials
│   ├── auth.svelte.ts       # Auth state ($state rune), login/logout
│   ├── theme.svelte.ts      # 6 themes, localStorage + server sync
│   └── components/
│       ├── TiptapEditor.svelte    # Rich text editor (Tiptap)
│       ├── EditorToolbar.svelte   # Toolbar for formatting
│       ├── CommandPalette.svelte  # ⌘⇧P command palette
│       ├── MediaGallery.svelte    # Grid/list of uploaded media
│       └── MediaPicker.svelte     # Modal to insert media into editor
└── routes/
    ├── +layout.svelte       # Auth guard, redirects to /login
    ├── +page.svelte         # Main app: sidebar, editor, media tab
    └── login/+page.svelte   # Password login form
```

## Data Flow

### Authentication

1. User submits password on `/login`
2. Frontend POSTs to `/api/auth/login` with `{ password }`
3. Backend verifies against bcrypt hash from `ADMIN_PASSWORD_HASH` env var
4. If valid, backend sets a 30-day JWT httpOnly cookie (`token`)
5. All subsequent API requests include the cookie automatically (`credentials: "include"`)
6. Backend `auth_middleware` decodes and validates the JWT on protected routes

### Notes CRUD

1. User types in the Tiptap editor (JSON-based rich text)
2. On save (auto-save after 2s debounce, or manual Cmd+S), frontend PUTs to `/api/notes/{slug}`
3. Backend updates the note in SQLite, scans content for `[[]]` link patterns, updates `note_links` table
4. Sidebar re-fetches the note list via `GET /api/notes`

### Wiki Links

- When a note is saved, the backend scans content for `[[slug]]` patterns
- Found links populate the `note_links` table (source → target, or source → NULL for unresolved)
- When fetching a note (`GET /api/notes/{slug}`), the backend includes backlinks (notes that link to this one)
- Unresolved links become active when a note with that slug is created

### Search

- SQLite FTS5 virtual table (`notes_fts`) mirrors `notes.title` and `notes.content`
- Triggers on INSERT/UPDATE/DELETE keep the FTS index in sync
- `GET /api/search?q=...` returns matching notes with highlighted snippets

### Media

- Files are uploaded via multipart form to `/api/media`
- Backend generates a UUID filename, uploads to Cloudflare R2
- Media metadata (filename, mime type, size, note association, tags) stored in SQLite
- File serving: `GET /api/media/{id}/file` streams from R2 with correct Content-Type
- Media can be imported from a URL via `/api/media/from-url`

### Theme Preferences

- Frontend stores theme in `localStorage` for instant load
- On layout mount, fetches theme from `/api/preferences` (server wins if different)
- Theme changes sync both localStorage and server via `PUT /api/preferences`
- Themes are defined in CSS custom properties under `[data-theme="..."]` selectors

## Database Schema

```sql
notes (id TEXT PK, title TEXT, slug TEXT UNIQUE, content TEXT, created_at, updated_at)
tags (id TEXT PK, name TEXT UNIQUE)
note_tags (note_id FK → notes, tag_id FK → tags, composite PK)
note_links (source_note_id FK → notes, target_note_id FK → notes NULLABLE, composite PK)
media (id TEXT PK, filename, original_name, mime_type, size, note_id FK NULLABLE, created_at)
media_tags (media_id FK → media, tag_id FK → tags, composite PK)
preferences (key TEXT PK, value TEXT)
notes_fts (FTS5 virtual table on title + content)
_migrations (name TEXT PK, applied_at)    -- migration tracking
```

All primary keys are UUIDs (v4). Timestamps are ISO 8601 text (SQLite has no native datetime type). Foreign keys cascade on delete where appropriate. `note_links.target_note_id` is nullable  --  links to not-yet-created notes remain pending and resolve when the target is created.

## Design Decisions

**Database abstraction:** A `Database` enum wraps either `SqlxDatabase` (local) or `LibsqlDatabase` (Turso). The choice is made at startup based on `DATABASE_URL`: `libsql://` URLs use Turso, everything else uses local SQLite. The rest of the application is completely agnostic  --  all queries go through the same `db.execute()`, `db.row_all()`, etc. methods.

## Design Decisions

**SQLite over PostgreSQL:** Simpler self-hosting story. One file, no separate database server. FTS5 provides adequate full-text search for a single user.

**Tiptap over Milkdown/ProseMirror directly:** Tiptap provides a higher-level API over ProseMirror with rich extensions (tables, task lists, code blocks, images). Markdown import/export via `tiptap-markdown`.

**SPA over SSR:** The app is fully client-side (`ssr = false` in layout). No server rendering needed since all data is behind authentication and highly interactive (editor, command palette).

**JWT in httpOnly cookies over Bearer tokens:** Simpler for a SPA  --  no need to manage tokens in JavaScript. Cookie is sent automatically. Protects against XSS token theft.

**Single user only:** Eliminates complexity of user management, permissions, sharing. The admin password is configured via environment variable.
