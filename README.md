![lychee](assets/banner.png)

![Rust](https://github.com/hello-rust/lychee/workflows/Rust/badge.svg)

...because who says I can't write yet another link checker.

## What?

This thing was created from [Hello Rust Episode
10](https://hello-rust.show/10/). It's a link checker that treats Github links
specially by using a `GITHUB_TOKEN` to avoid getting blocked less by the rate limiter.

## Why?

The existing link checkers were not flexible enough for my use-case.
lychee can...

- run requests concurrently (fully async Rust)
- handle links inside Markdown, HTML, and other documents
- handle chunked encodings
- handle gzip compression
- fake user agents (required for some firewalls)
- skip non-links like anchors or relative URLs
- exclude some websites with regular expressions
- handle a configurable number of redirects
- disguise as a different user agent (like curl)
- optionally ignore SSL certificate errors
- run with a low memory/CPU footprint

SOON:

- automatically retry and backoff
- check relative and absolute paths
- limit schema
- check multiple files
- support input files using wildcards
- set timeout for HTTP requests in seconds. Disabled by default.
- support checking links from any website URL
- accept custom headers (see https://github.com/rust-lang/crates.io/issues/788)
- use `HEAD` requests instead of `GET` to avoid network I/O
- show the progress
- show final status

## How?

```
cargo install lychee
```

Set an environment variable with your token like so `GITHUB_TOKEN=xxxx`.

Run it inside a repository with a `README.md` or specify a different Markdown
file with

```
lychee --input <yourfile.md>
```

## Thanks

...to my Github sponsors and Patreon sponsors for supporting these projects. If
you want to help out as well, [go here](https://github.com/sponsors/mre/).
