syntax = "docker/dockerfile:1.5"

FROM --platform=$BUILDPLATFORM rust:1-bullseye as builder

WORKDIR /app

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
COPY app app/
COPY crates crates/
COPY config config/
COPY deploy deploy/
COPY openapi openapi/
COPY .cargo .cargo

RUN cargo fetch --workspace

# Build release binaries
RUN cargo build --release --workspace --locked

FROM gcr.io/distroless/base-debian12:nonroot

LABEL maintainer="stephane-segning <selastlambou@gmail.com>"
LABEL org.opencontainers.image.description="Backend for Keycloak Keybound x Fineract"

WORKDIR /app

COPY --from=builder /app/target/release/backend /usr/local/bin/backend
COPY --from=builder /app/target/release/health /usr/local/bin/health

RUN mkdir -p /etc/ssl/certs
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

EXPOSE 3000 3443

HEALTHCHECK --interval=30s --timeout=3s --start-period=1s --retries=3 \
    CMD ["/usr/local/bin/health", "-r", "3000"]

ENV RUST_LOG=info

ENTRYPOINT ["backend"]
