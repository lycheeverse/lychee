FROM alpine:latest AS builder
WORKDIR /builder

ARG LYCHEE_VERSION="latest"

RUN apk update \
    && apk add --no-cache ca-certificates jq wget \
    && case $(arch) in \
      "x86_64") \
        wget -4 -q -O - https://github.com/lycheeverse/lychee/releases/$LYCHEE_VERSION/download/lychee-x86_64-unknown-linux-musl.tar.gz | tar -xz lychee \
      ;; \
      "aarch64") \
        wget -4 -q -O - https://github.com/lycheeverse/lychee/releases/$LYCHEE_VERSION/download/lychee-arm-unknown-linux-musleabihf.tar.gz | tar -xz lychee \
      ;; \
    esac \
    && chmod +x lychee

FROM alpine:latest
RUN apk add --no-cache ca-certificates tzdata \
    && addgroup -S lychee \
    && adduser -D -G lychee -S lychee

COPY --from=builder /builder/lychee /usr/local/bin/lychee
# Run as non-root user
USER lychee
ENTRYPOINT [ "/usr/local/bin/lychee" ]
