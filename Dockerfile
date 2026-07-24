FROM rust:1.81-bookworm AS builder
RUN apt-get update && apt-get install -y gcc make cmake clang llvm pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
RUN mkdir src tests && echo "fn main() {}" > src/main.rs
RUN cargo build --release 2>/dev/null || true
RUN rm -rf src tests
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/* && \
    groupadd --gid 65532 thebridge && \
    useradd --uid 65532 --gid thebridge --shell /usr/sbin/nologin --no-create-home thebridge && \
    mkdir -p /data/wal /data/iso20022 && \
    chown -R thebridge:thebridge /data
COPY --from=builder /app/target/release/the-bridge-matching-engine /app/
USER thebridge
EXPOSE 3001
ENV RUST_LOG=info
ENTRYPOINT ["/app/the-bridge-matching-engine"]
