# syntax=docker/dockerfile:1

# Stage 1: build the web frontend
FROM node:22-bookworm AS frontend
WORKDIR /app
ENV NODE_OPTIONS=--max-old-space-size=4096
COPY package*.json ./
RUN npm ci
COPY . .
RUN VITE_TAURI=false npm run build:web

# Stage 2: build the Rust server binary
FROM rust:bookworm AS builder
WORKDIR /usr/src/agent-ide
RUN apt-get update && apt-get install -y --no-install-recommends \
      libssl-dev \
      libgit2-dev \
      pkg-config \
      libgtk-3-dev \
      libwebkit2gtk-4.1-dev \
      libappindicator3-dev \
      librsvg2-dev \
    && rm -rf /var/lib/apt/lists/*
COPY src-tauri/Cargo.toml src-tauri/Cargo.lock ./
COPY src-tauri/. .
COPY --from=frontend /app/dist ./dist
RUN cargo build --release --bin agent-ide-server

# Stage 3: minimal runtime image
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
      git \
      openssh-client \
      ca-certificates \
      libgtk-3-0 \
      libwebkit2gtk-4.1-0 \
      libappindicator3-1 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /usr/src/agent-ide/target/release/agent-ide-server /app/
COPY --from=frontend /app/dist /app/dist
EXPOSE 3000
ENV AGENT_IDE_PORT=3000
CMD ["/app/agent-ide-server"]
