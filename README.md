# openslate

A self-hosted markdown note-taking app. Fast, simple, private. Access your notes from any device.

**Single user only.** No sign-ups, no sharing, no complexity. Just notes.

## Features

- **Rich text editing** - Tiptap editor with markdown support, syntax highlighting, tables, task lists
- **Tags** - Organize notes with tags, filter by tag
- **Keyboard shortcuts** - Cmd+Shift+P command palette, full keyboard nav
- **Full-text search** - Search across all notes instantly (SQLite FTS5)
- **Media** - Upload images and files, stored in Cloudflare R2
- **Six themes** - Light, dark, sepia, nord, monokai, tokyo-night
- **Simple auth** - Single password, log in from any device (JWT cookies)
- **Self-contained** - One binary + SQLite

## Stack

- **Frontend:** SvelteKit + Tailwind CSS + Tiptap
- **Backend:** Rust (Axum)
- **Database:** SQLite  --  local file or [Turso](https://turso.tech/) (distributed, zero-ops)
- **Auth:** JWT via httpOnly cookies
- **Media:** Cloudflare R2

## Documentation

Full documentation is in the [docs/](docs/index.md) directory:

- [Setup Guide](docs/setup.md)  --  Prerequisites, env vars, running locally
- [Architecture](docs/architecture.md)  --  Project structure, data flow, design decisions
- [Features](docs/features.md)  --  Editor, tags, search, media, themes
- [API Reference](docs/api-reference.md)  --  Full REST API docs
- [Deployment](docs/deployment.md)  --  Coming soon

## Deploy

### DigitalOcean (or any Ubuntu VPS)

**1. Create a Droplet**

Choose Ubuntu 24.04, the cheapest plan ($4/mo) is plenty. Under "Additional options", paste the contents of [`scripts/cloud-init.yaml`](scripts/cloud-init.yaml) into the **User data** field. Create the Droplet.

Wait 2–3 minutes for the first build. Your app is live at `http://<droplet-ip>:8080`.

**2. Configure (optional)**

SSH into the Droplet and edit `/opt/openslate/.env`:

```bash
DOMAIN=notes.example.com       # enables automatic HTTPS via Caddy
R2_BUCKET=my-bucket            # enables image/file uploads
R2_ACCOUNT_ID=abc123
R2_ACCESS_KEY=abc123
R2_SECRET_KEY=abc123

# Or switch to Turso (distributed SQLite):
# DATABASE_URL=libsql://your-db.turso.io
# TURSO_AUTH_TOKEN=your-token-here
# BUILD_FEATURES=backend-turso
```
For Turso, after setting the env vars, also rebuild:
```bash
cd /opt/openslate && docker compose build && docker compose up -d
```

**3. Enable HTTPS (if you set DOMAIN)**

Edit `/opt/openslate/docker-compose.yml`  --  comment out `8080:8080` and uncomment `80:80` + `443:443`. Then point your domain's DNS A record to the Droplet IP.

```bash
cd /opt/openslate && docker compose up -d
```

Caddy auto-provisions a Let's Encrypt TLS certificate. Access your notes at `https://notes.example.com`.

## Development

```bash
# Local SQLite (default):
cd api && cargo run

# Turso (distributed):
cd api && cargo run --features backend-turso
```

```bash
cd web && bun run dev
```

## LICENSE
MIT
