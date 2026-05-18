# ──── Stage 1: build radar-ui static bundle ───────────────────────────────────
FROM node:22-alpine AS ui-builder
WORKDIR /workspace
RUN corepack enable
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY radar-ui/package.json ./radar-ui/
RUN pnpm install --frozen-lockfile --filter radar-ui...
COPY radar-ui ./radar-ui
RUN pnpm --filter radar-ui build

# ──── Stage 2: build radar-api binary ─────────────────────────────────────────
FROM rust:1.78-slim-bookworm AS api-builder
WORKDIR /workspace
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release -p radar-api

# ──── Stage 3: minimal runtime image ──────────────────────────────────────────
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=api-builder /workspace/target/release/radar-api /usr/local/bin/
COPY --from=ui-builder /workspace/radar-ui/dist /srv/ui
EXPOSE 8080
CMD ["radar-api", "--static-dir", "/srv/ui"]
