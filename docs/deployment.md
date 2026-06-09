# Deployment

OpenSlate ships as a single Docker container bundling the Rust backend, the SvelteKit frontend, and Caddy as the reverse proxy.

## Option A: Docker with local SQLite (simplest)

This stores the database on a Docker volume on the host. Good for single-server deployments.

### 1. Configure environment

```bash
cp .env.example .env
```

Edit `.env`:

```env
JWT_SECRET=your-random-secret
DOMAIN=your-domain.com          # optional  --  enables HTTPS via Let's Encrypt

# Optional  --  Cloudflare R2 for media uploads
# R2_BUCKET=...
# R2_ACCOUNT_ID=...
# R2_ACCESS_KEY=...
# R2_SECRET_KEY=...
```

### 2. Build and start

```bash
docker compose up -d
```

The app is available on `http://localhost:8080` (or your domain with HTTPS).

The SQLite database is persisted in the `data` Docker volume. Back it up with:

```bash
docker compose cp openslate:/data/data.db ./backup.db
```

### 3. Update

```bash
git pull
docker compose up -d --build
```

## Option B: Docker with Turso (distributed SQLite)

Use Turso instead of a local volume  --  no persistent Docker volumes needed, no backup concerns, runs identically across multiple machines.

### 1. Create a Turso database

```bash
brew install tursodatabase/tap/turso
turso auth signup
turso db create openslate
turso db show openslate --url       # → libsql://your-db.turso.io
turso db tokens create openslate    # → your-auth-token
```

### 2. Configure environment

```bash
cp .env.example .env
```

Edit `.env`:

```env
JWT_SECRET=your-random-secret
DOMAIN=your-domain.com

# Turso config
DATABASE_URL=libsql://your-db.turso.io
TURSO_AUTH_TOKEN=your-token-here

# Optional  --  Cloudflare R2
# R2_BUCKET=...
```

### 3. Build and start

The Docker image must be built with the Turso feature:

```bash
docker compose build --build-arg BUILD_FEATURES=backend-turso
docker compose up -d
```

For convenience, you can set a default in `docker-compose.yml` or use a `docker-compose.override.yml`:

```yaml
# docker-compose.override.yml
services:
  openslate:
    build:
      args:
        BUILD_FEATURES: backend-turso
```

## Option C: VPS one-click (DigitalOcean)

Use the cloud-init script at `scripts/cloud-init.yaml`:

1. Create a new Droplet on DigitalOcean
2. Paste the contents of `scripts/cloud-init.yaml` into the **User Data** field
3. Choose Ubuntu 24.04, deploy

After boot, the app is running on `http://<droplet-ip>:8080`.

**For local SQLite:** Everything is set up automatically  --  no further steps needed.

**For Turso:** SSH into the server and edit `/opt/openslate/.env`:

```bash
ssh root@<droplet-ip>
# Edit /opt/openslate/.env:
#   DATABASE_URL=libsql://your-db.turso.io
#   TURSO_AUTH_TOKEN=your-token-here
# Then set the build arg and restart:
cd /opt/openslate
echo 'BUILD_FEATURES=backend-turso' >> .env
docker compose build --build-arg BUILD_FEATURES=backend-turso
docker compose up -d
```

## Environment reference

| Variable | Required | Description |
|----------|----------|------------|
| `JWT_SECRET` | Yes | Random secret for signing JWT tokens |
| `DATABASE_URL` | Yes (Turso) | Turso libsql:// URL. Defaults to `sqlite:/data/data.db?mode=rwc` if unset |
| `TURSO_AUTH_TOKEN` | Yes (Turso) | Turso auth token |
| `DOMAIN` | No | Your domain  --  enables automatic HTTPS via Let's Encrypt |
| `R2_BUCKET` | No | Cloudflare R2 bucket for media uploads |
| `R2_ACCOUNT_ID` | No | Cloudflare account ID |
| `R2_ACCESS_KEY` | No | R2 access key |
| `R2_SECRET_KEY` | No | R2 secret key |

## How it works

The Docker image contains:

- **api**  --  Rust Axum server on port 3001
- **Caddy**  --  Reverse proxy on port 8080 (or 80/443 with HTTPS):
  - `/api/*` → proxied to the Rust backend
  - `/*` → SvelteKit static files with SPA fallback
- **docker-entrypoint.sh**  --  Starts the API, waits for health check, then starts Caddy
