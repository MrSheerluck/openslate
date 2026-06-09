# Setup Guide

This guide walks through setting up OpenSlate for local development.

## Prerequisites

- **Rust** (latest stable)  --  [rustup.rs](https://rustup.rs/)
- **Bun** v1.x  --  [bun.sh](https://bun.sh/) (or Node.js + npm, but Bun is preferred)
- **Cloudflare R2** account (optional  --  only needed for media uploads)

## Clone and Install

```bash
git clone https://github.com/MrSheerluck/openslate.git
cd openslate
```

## Backend Setup

### 1. Choose your database

OpenSlate supports two database backends with zero code changes  --  just pick one:

| Backend | Database | When to use |
|---------|----------|------------|
| **Local SQLite** (default) | File on disk (`data.db`) | Development, single-server self-hosting |
| **Turso** | Distributed SQLite (libsql) | Multi-region, edge deployments, zero-ops DB |

**For local SQLite** (default  --  no setup needed):
```bash
cp api/.env.example api/.env
# DATABASE_URL is already set to sqlite:data.db?mode=rwc
```

**For Turso** (distributed SQLite):
```bash
# 1. Install Turso CLI
brew install tursodatabase/tap/turso

# 2. Sign up, create a database, and get credentials
turso auth signup
turso db create openslate
turso db show openslate --url       # → libsql://your-db.turso.io
turso db tokens create openslate    # → your-auth-token

# 3. Configure .env
cp api/.env.example api/.env
# Edit api/.env:
#   DATABASE_URL=libsql://your-db.turso.io
#   TURSO_AUTH_TOKEN=your-token-here
```

### 2. Configure remaining environment variables

Edit `api/.env` and fill in the values:

| Variable | Description |
|----------|------------|
| `DATABASE_URL` | SQLite path or Turso libsql:// URL (see above) |
| `TURSO_AUTH_TOKEN` | Required only when using Turso |
| `HOST` | Bind address. Default: `0.0.0.0` |
| `PORT` | Server port. Default: `3001` |
| `FRONTEND_URL` | Frontend origin for CORS. Default: `http://localhost:5173` |
| `JWT_SECRET` | Random string for signing JWT tokens. Generate with `openssl rand -base64 32` |
| `ADMIN_PASSWORD` | Password for the admin user (set on first run) |
| `R2_BUCKET` | Cloudflare R2 bucket name (optional) |
| `R2_ACCOUNT_ID` | Cloudflare account ID (optional) |
| `R2_ACCESS_KEY` | R2 access key (optional) |
| `R2_SECRET_KEY` | R2 secret key (optional) |

### 3. Run the backend

```bash
cd api

# For local SQLite (default):
cargo run

# For Turso:
cargo run --features backend-turso
```

The API starts on `http://localhost:3001`. Verify with:

```bash
curl http://localhost:3001/api/health
# → {"status":"ok"}
```

On first run, database migrations run automatically  --  the database file (`data.db`) is created in the `api/` directory (for local SQLite) or on Turso.

## Frontend Setup

### 1. Configure environment

```bash
cp web/.env.example web/.env
```

The default value (`VITE_API_URL=http://localhost:3001`) points at the local backend. Adjust if needed.

### 2. Install dependencies and run

```bash
cd web
bun install
bun run dev
```

The frontend starts on `http://localhost:5173`. Open it in your browser  --  you'll see the login page. Enter the password you configured in the backend `.env`.

## Verifying Everything Works

1. Backend: `curl http://localhost:3001/api/health` returns `{"status":"ok"}`
2. Frontend: `http://localhost:5173` shows the login page
3. Login with your configured password
4. Create a note  --  it should save and appear in the sidebar
5. Upload an image in the Media tab  --  it should upload to R2 and appear in the gallery

## Project Structure

```
openslate/
├── api/                # Rust backend
│   ├── src/
│   │   ├── db/         # Database abstraction (sqlx + libsql backends)
│   │   ├── main.rs     # Entry point, router, CORS
│   │   ├── config.rs   # Environment variable loading
│   │   ├── auth.rs     # JWT middleware, login/logout
│   │   ├── notes.rs    # Note CRUD, wiki links, tags
│   │   ├── media.rs    # Media upload/serve, R2 integration
│   │   └── preferences.rs
│   ├── migrations/     # SQL migrations (auto-run on startup)
│   ├── .env.example    # Environment template
│   └── Cargo.toml
├── web/                # SvelteKit frontend
│   ├── src/
│   │   ├── lib/        # Shared modules (api client, auth, theme, components)
│   │   └── routes/     # SvelteKit pages
│   ├── .env.example
│   └── package.json
└── docs/               # Documentation (you are here)
```
