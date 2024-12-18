FROM debian:bookworm-slim AS builder
WORKDIR /builder

ARG LYCHEE_VERSION="latest"

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y \
    --no-install-recommends \
    ca-certificates \
    jq \
    wget \
    && case $(dpkg --print-architecture) in \
      "amd64") \
        wget -q -O - https://github.com/lycheeverse/lychee/releases/download/$LYCHEE_VERSION/lychee-x86_64-unknown-linux-gnu.tar.gz | tar -xz lychee \
      ;; \
      "arm64") \
        wget -q -O - https://github.com/lycheeverse/lychee/releases/download/$LYCHEE_VERSION/lychee-aarch64-unknown-linux-gnu.tar.gz | tar -xz lychee \
      ;; \
    esac \
    && chmod +x lychee

FROM debian:bookworm-slim

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y \
    --no-install-recommends \
    ca-certificates \
    tzdata \
    && rm -rf /var/cache/debconf/* \
    # Clean and keep the image small. This should not
    # be necessary as the debian-slim images have an
    # auto clean mechanism but we may rely on other
    # images in the future (see:
    # https://github.com/debuerreotype/debuerreotype/blob/master/scripts/debuerreotype-minimizing-config).
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /builder/lychee /usr/local/bin/lychee
ENTRYPOINT [ "/usr/local/bin/lychee" ]
