FROM debian:bookworm-slim AS builder
WORKDIR /builder

ARG LYCHEE_VERSION="latest"

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        ca-certificates \
        jq \
        wget \
    && rm -rf /var/lib/apt/lists/* \
    && ARCH=$(case $(dpkg --print-architecture) in \
        "amd64") echo "x86_64";; \
        "arm64") echo "aarch64";; \
        *) echo "Unsupported architecture" && exit 1;; \
        esac) \
    && BASE_URL=$(case $LYCHEE_VERSION in \
        "latest" | "nightly") echo "https://github.com/lycheeverse/lychee/releases/latest/download";; \
        *) echo "https://github.com/lycheeverse/lychee/releases/download/$LYCHEE_VERSION";; \
        esac) \
    && wget -q -O - "$BASE_URL/lychee-$ARCH-unknown-linux-gnu.tar.gz" | tar -xz lychee \
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
