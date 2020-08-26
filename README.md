![lychee](assets/banner.png)

![Rust](https://github.com/hello-rust/lychee/workflows/Rust/badge.svg)

...because who says I can't write yet another link checker?

## What?

This thing was created from [Hello Rust Episode
10](https://hello-rust.show/10/). It's a link checker that treats Github links
specially by using a `GITHUB_TOKEN` to avoid getting blocked by the rate
limiter.

TODO: Add screenshots here

## Why?

The existing link checkers were not flexible enough for my use-case. lychee
runs all requests fully asynchronously and has a low memory/CPU footprint.

lychee can...

- handle links inside Markdown, HTML, and other documents
- handle chunked encodings
- handle gzip compression
- fake user agents (required for some firewalls)
- skip non-links like anchors or relative URLs
- exclude some websites with regular expressions
- handle a configurable number of redirects
- disguise as a different user agent (like curl)
- optionally ignore SSL certificate errors (`--insecure`)
- check multiple files at once (supports globbing)
- support checking links from any website URL
- limit scheme (e.g. only check HTTPS links with "https")
- accept custom headers (e.g. for cases like https://github.com/rust-lang/crates.io/issues/788)
- show final summary/statistics
- optionally use `HEAD` requests instead of `GET`
- show colored output
- filter based on status codes (https://github.com/tcort/markdown-link-check/issues/94)
  (e.g. `--accept 200,204`)
- accept a request timeout (`--timeout`) in seconds. Default is 20s. Set to 0 for no timeout.
- check e-mail links using [check-if-mail-exists](https://github.com/amaurymartiny/check-if-email-exists)

SOON:

- automatically retry and backoff
- check relative (`base-url` to set project root)
- show the progress interactively with progress bar and in-flight requests (`--progress`)
- usable as a library (https://github.com/raviqqe/liche/issues/13)
- exclude private domains (https://github.com/appscodelabs/liche/blob/a5102b0bf90203b467a4f3b4597d22cd83d94f99/url_checker.go)
- recursion
- extended statistics: request latency
- use colored output (https://crates.io/crates/colored)

## Users

- SOON: https://github.com/analysis-tools-dev/static-analysis

## How?

```
cargo install lychee
```

Set an environment variable with your token like so `GITHUB_TOKEN=xxxx`.

Run it inside a repository with a `README.md` or specify a different Markdown
file with

```
lychee <yourfile>
```

## Comparison

Collecting other link checkers here to crush them in comparison. :P

- https://github.com/dkhamsing/awesome_bot
- https://github.com/tcort/markdown-link-check
- https://github.com/raviqqe/liche
- https://github.com/raviqqe/muffet

## Thanks

...to my Github sponsors and Patreon sponsors for supporting these projects. If
you want to help out as well, [go here](https://github.com/sponsors/mre/).
