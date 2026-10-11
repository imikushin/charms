# Reproducible RISC-V builds of the charms-spell-checker and charms-proof-wrapper guests.
#
# The toolchain image is published for linux/amd64 and linux/arm64. The tag is
# the full commit of the guest-builder branch that published it.
#
#   charms-spell-checker/build-guests.sh

FROM ghcr.io/charmsdev/guest-builder:5b1eae7e339ab66544700be56774e6096d45937b

WORKDIR /src
COPY . .

RUN cd charms-spell-checker \
    && cargo prove build --locked --output-directory=/out \
    && test -s /out/charms-spell-checker \
    && rm -rf /src/charms-spell-checker/target

RUN cargo run --profile test --locked --manifest-path /src/charms-proof-wrapper/vk/Cargo.toml -- \
        checker \
        /out/charms-spell-checker \
        /src/charms-proof-wrapper/src/lib.rs \
        /out/charms-spell-checker-vk.bin \
    && test -s /out/charms-spell-checker-vk.bin \
    && cp /src/charms-proof-wrapper/src/lib.rs /out/charms-proof-wrapper-lib.rs

RUN cargo prove build --locked -p charms-proof-wrapper --output-directory=/out \
    && test -s /out/charms-proof-wrapper \
    && rm -rf /src/target

RUN cargo run --profile test --locked --manifest-path /src/charms-proof-wrapper/vk/Cargo.toml -- \
        wrapper \
        /out/charms-proof-wrapper \
        /src/charms-lib/src/lib.rs \
        /out/charms-proof-wrapper-vk.bin \
    && test -s /out/charms-proof-wrapper-vk.bin \
    && cp /src/charms-lib/src/lib.rs /out/charms-lib.rs
