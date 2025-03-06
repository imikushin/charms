FROM golang AS base
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
RUN go version && rustc --version && cargo --version
RUN apt update && apt install -y build-essential pkg-config clang libclang-dev libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app

FROM base AS builder
COPY . .
RUN cargo install --locked --path . --bin charms

FROM ubuntu AS runtime
COPY --from=builder /root/.cargo/bin/charms /usr/local/bin
CMD ["charms", "server"]
