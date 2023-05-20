FROM alpine:latest as builder
WORKDIR /builder

RUN apk update \
    && apk add --no-cache ca-certificates jq wget \
    && case $(arch) in \
      "x86_64") \
        wget -4 -q -O - "$(wget -4 -q -O- https://api.github.com/repos/lycheeverse/lychee/releases/latest \
        | jq -r '.assets[].browser_download_url' \
        | grep x86_64-unknown-linux-musl)" | tar -xz lychee \
      ;; \
      "aarch64") \
        wget -4 -q -O - "$(wget -4 -q -O- https://api.github.com/repos/lycheeverse/lychee/releases/latest \
        | jq -r '.assets[].browser_download_url' \
        | grep arm-unknown-linux-musleabihf)" | tar -xz lychee \
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
