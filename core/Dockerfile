# Container image for the codel00p-cloud control-plane HTTP service.
# Build context is the Cargo workspace root (core/), so the path-dependency
# crates and Cargo.lock are available to the release build.

FROM rust:1-bookworm AS builder
WORKDIR /build
# Postgres backend pulls in native-tls (OpenSSL) for TLS to Neon/Fly/Supabase.
RUN apt-get update \
  && apt-get install -y --no-install-recommends pkg-config libssl-dev \
  && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release -p codel00p-cloud --features postgres

FROM debian:bookworm-slim
# ca-certificates: outbound TLS to Clerk JWKS. libssl3: runtime for native-tls.
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates libssl3 \
  && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/codel00p-cloud /usr/local/bin/codel00p-cloud

# Hosts (Fly/Render/Railway) inject PORT; the service binds 0.0.0.0:$PORT.
ENV PORT=8080
EXPOSE 8080
CMD ["codel00p-cloud"]
