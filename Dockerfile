FROM rust AS base
WORKDIR /app

FROM base AS builder
COPY . .
RUN cargo install --locked --path . --bin charms

FROM ubuntu AS runtime
COPY --from=builder /usr/local/cargo/bin/charms /usr/local/bin
CMD ["charms", "server"]
