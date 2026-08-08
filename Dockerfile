FROM rust:1.82-slim as builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY core/ core/
COPY flash-loan/ flash-loan/
COPY arbitrage/ arbitrage/
COPY mev-protection/ mev-protection/
COPY cross-venue-arb/ cross-venue-arb/
COPY super-arb/ super-arb/
COPY chaos/ chaos/
COPY integration/ integration/
COPY src/ src/

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
RUN cargo build --release --bin api-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/api-server /usr/local/bin/api-server

EXPOSE 3001
ENTRYPOINT ["/usr/local/bin/api-server"]
