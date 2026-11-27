FROM rust:1.86-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/fluxpay /usr/local/bin/fluxpay
ENV FLUXPAY_HOST=0.0.0.0 FLUXPAY_PORT=8080 FLUXPAY_DATABASE_URL=sqlite://fluxpay.db
EXPOSE 8080
CMD ["fluxpay"]
