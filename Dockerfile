FROM rust:1.48 as builder

RUN USER=root cargo new --bin lychee
WORKDIR /lychee
COPY Cargo.toml Cargo.toml
RUN cargo build --release
RUN rm src/*.rs

ADD . ./

RUN rm ./target/release/deps/lychee*
RUN cargo build --release


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