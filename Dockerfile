FROM rust:latest as builder

RUN USER=root cargo new --bin lychee
WORKDIR /lychee

# Just copy the Cargo.toml files and trigger
# a build so that we compile our dependencies only.
# This way we avoid layer cache invalidation
# if our dependencies haven't changed,
# resulting in faster builds.
COPY lychee-bin/Cargo.toml lychee-bin/Cargo.toml
COPY lychee-lib/Cargo.toml lychee-lib/Cargo.toml
RUN cargo build --release \
    && rm src/*.rs

# Copy the source code and run the build again.
# This should only compile lychee itself as the
# dependencies were already built above.
COPY . ./
RUN rm ./target/release/deps/lychee* \
    && cargo build --release

# Our production image starts here, which uses
# the files from the builder image above.
FROM debian:bullseye-slim

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y \
    --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/cache/debconf/* \
    # Clean and keep the image small. This should not
    # be necessary as the debian-slim images have an
    # auto clean mechanism but we may rely on other
    # images in the future (see:
    # https://github.com/debuerreotype/debuerreotype/blob/master/scripts/debuerreotype-minimizing-config).
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /lychee/target/release/lychee /usr/local/bin/lychee
ENTRYPOINT [ "lychee" ]
