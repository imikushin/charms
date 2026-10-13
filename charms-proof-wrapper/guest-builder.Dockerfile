# Toolchain for reproducible RISC-V builds of the zkVM guests.
#
# Published for linux/amd64 and linux/arm64 on pushes to the guest-builder branch.
# The tag is that push's full commit:
#   ghcr.io/charmsdev/guest-builder:<commit>
#
# SP1 6.8.1 matches sp1-zkvm. Ubuntu's clang and the RISC-V gcc from
# `sp1up --c-toolchain` compile blst and secp256k1 for the spell checker.
# spell-checker-vk is built from charms-proof-wrapper/vk against that same
# sp1-sdk and installed at /usr/local/bin/spell-checker-vk.
#
# Build locally (Docker picks the host architecture):
#   docker build -f charms-proof-wrapper/guest-builder.Dockerfile -t ghcr.io/charmsdev/guest-builder .
# The guest Dockerfile pins a published commit tag of this image.

FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:/root/.sp1/bin:${PATH}

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        clang \
        cmake \
        curl \
        git \
        libssl-dev \
        libprotobuf-dev \
        pkg-config \
        protobuf-compiler \
        xz-utils \
    && rm -rf /var/lib/apt/lists/*

# Host toolchain matches rust-toolchain.toml. `cargo prove` compiles the guest
# with the succinct toolchain installed next.
RUN curl --proto '=https' --tlsv1.2 --retry 10 --retry-connrefused -fsSL https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain 1.96 --profile minimal

RUN curl -fsSL https://sp1up.succinct.xyz | bash \
    && sp1up -v 6.8.1 -c \
    && rustup toolchain list \
    && cargo prove --version \
    && riscv64-unknown-elf-gcc --version

# The test profile is the one in charms-proof-wrapper/vk (opt-level 1, no LTO).
# Cargo stores that profile in target/debug. Debuginfo is stripped so the image
# does not keep sp1-sdk's symbols. Setup of each ELF still runs in the guest build.
COPY charms-proof-wrapper/vk/Cargo.toml charms-proof-wrapper/vk/Cargo.lock /tmp/spell-checker-vk/
COPY charms-proof-wrapper/vk/src /tmp/spell-checker-vk/src
RUN cargo build --locked --profile test --manifest-path /tmp/spell-checker-vk/Cargo.toml \
        --config 'profile.test.strip="debuginfo"' \
    && install -m 755 /tmp/spell-checker-vk/target/debug/spell-checker-vk /usr/local/bin/spell-checker-vk \
    && rm -rf /tmp/spell-checker-vk

WORKDIR /src
