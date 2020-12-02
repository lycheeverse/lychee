FROM rust:latest as builder

RUN USER=root cargo new --bin lychee
WORKDIR /lychee

# Just copy the Cargo.toml and trigger a build so 
# that we compile our dependencies only.
# This way we avoid layer cache invalidation
# if our dependencies haven't changed,
# resulting in faster builds.
COPY Cargo.toml Cargo.toml
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
ARG APP=/usr/src/lychee

RUN apt-get update \
    && apt-get install -y ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*

ENV TZ=Etc/UTC \
    APP_USER=lychee

RUN groupadd $APP_USER \
    && useradd -g $APP_USER $APP_USER \
    && mkdir -p ${APP}

COPY --from=builder /lychee/target/release/lychee ${APP}/lychee

RUN chown -R $APP_USER:$APP_USER ${APP}

USER $APP_USER
WORKDIR ${APP}

ENTRYPOINT [ "./lychee" ]
