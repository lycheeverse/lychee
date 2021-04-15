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
RUN cargo build --release
RUN rm src/*.rs

# Copy the source code and run the build again.
# This should only compile lychee itself as the
# dependencies were already built above.
ADD . ./
RUN rm ./target/release/deps/lychee*
RUN cargo build --release


# Our production image starts here, which uses 
# the files from the builder image above.
FROM debian:buster-slim

RUN apt-get update \
    && apt-get install -y ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /lychee/target/release/lychee /usr/local/bin/lychee
ENTRYPOINT [ "lychee" ]
