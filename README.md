![lychee](assets/banner.png)

![Rust](https://github.com/hello-rust/lychee/workflows/Rust/badge.svg)

...because who says I can't write yet another link checker?

## What?

This thing was created from [Hello Rust Episode
10](https://hello-rust.show/10/). It's a link checker that treats Github links
specially by using a `GITHUB_TOKEN` to avoid getting blocked by the rate
limiter.

![Lychee demo](assets/lychee.gif)

## Why?

The existing link checkers were not flexible enough for my use-case. lychee
runs all requests fully asynchronously and has a low memory/CPU footprint.

## Features

|                                | lychee | awesome_bot | muffet | broken-link-checker | linkinator |
| ------------------------------ | ------ | ----------- | ------ | ------------------- | ---------- |
| Language                       | Rust   | Ruby        | Go     | JS                  | TypeScript |
| Static binary                  | ☑      | ☒           | ☑      | ☒                   | ☒          |
| Async/Parallel                 | ☑      | ☑           | ☑      | ☑                   | ☑          |
| Markdown support               | ☑      | ☑           | ☒      | ☒                   | ☒          |
| HTML support                   | ☑      | ☒           | ☒      | ☑                   | ☑          |
| Plaintext support              | ☑      | ☒           | ☒      | ☒                   | ☒          |
| Website support                | ☑      | ☒           | ☑      | ☑                   | ☑          |
| Chunked encodings              | ☑      | ?           | ?      | ?                   | ?          |
| GZIP compression               | ☑      | ?           | ?      | ☑                   | ?          |
| Basic Auth                     | ☒      | ☒           | ☒      | ☑                   | ☒          |
| Custom user agent              | ☑      | ☒           | ☒      | ☑                   | ☒          |
| Relative URLs                  | ☒      | ☑           | ☒      | ☑                   | ☑          |
| Skip relative URLs             | ☑      | ☒           | ☒      | ?                   | ☒          |
| Include patterns               | ☒      | ☑           | ☒      | ☑                   | ☒          |
| Exclude patterns               | ☑      | ☒           | ☑      | ☑                   | ☑          |
| Handle redirects               | ☑      | ☑           | ☑      | ☑                   | ☑          |
| Ignore SSL                     | ☑      | ☑           | ☑      | ☒                   | ☒          |
| File globbing                  | ☑      | ☑           | ☒      | ☒                   | ☑          |
| Limit scheme (e.g. only HTTPS) | ☑      | ☒           | ☒      | ☑                   | ☒          |
| [Custom headers]               | ☑      | ☒           | ☑      | ☒                   | ☒          |
| Summary                        | ☑      | ☑           | ☑      | ?                   | ☑          |
| `HEAD` requests                | ☑      | ☑           | ☒      | ☑                   | ☑          |
| Colored output                 | ☑      | ?           | ☑      | ?                   | ☑          |
| [Filter on status code]        | ☑      | ☑           | ☒      | ☒                   | ☒          |
| Custom request timeout         | ☑      | ☑           | ☑      | ☒                   | ☑          |
| E-mail links                   | ☑      | ☒           | ☒      | ☒                   | ☒          |
| Progress bar                   | ☑      | ☑           | ☒      | ☒                   | ☒          |
| Retry and backoff              | ☑      | ☒           | ☒      | ☒                   | ☑          |
| Exclude private domains        | ☑      | ☒           | ☒      | ☒                   | ☒          |
| [Usable as a library]          | ☒      | ☑           | ☒      | ☑                   | ☑          |
| Silent mode                    | ☑      | ☒           | ☒      | ☒                   | ☑          |

## Planned features:

- lychee.toml
- report output in HTML, SQL, CSV, XML, JSON, YAML... format
- report extended statistics: request latency
- recursion
- use colored output (https://crates.io/crates/colored)
- skip duplicate urls

## Users

- SOON: https://github.com/analysis-tools-dev/static-analysis

## How?

```
cargo install lychee
```

Set an environment variable with your token like so `GITHUB_TOKEN=xxxx`.

Run it inside a repository with a `README.md` or specify a file with

```
lychee <yourfile>
```

## Comparison

Collecting other link checkers here to crush them in comparison. :P

- https://github.com/dkhamsing/awesome_bot
- https://github.com/tcort/markdown-link-check
- https://github.com/raviqqe/liche
- https://github.com/raviqqe/muffet
- https://github.com/stevenvachon/broken-link-checker
- https://github.com/JustinBeckwith/linkinator
- https://github.com/linkchecker/linkchecker
- https://github.com/dantleech/fink
- https://github.com/bartdag/pylinkvalidator
- https://github.com/victoriadrake/hydra-link-checker

## Thanks

...to my Github sponsors and Patreon sponsors for supporting these projects. If
you want to help out as well, [go here](https://github.com/sponsors/mre/).

[custom headers]: https://github.com/rust-lang/crates.io/issues/788)
[filter on status code]: https://github.com/tcort/markdown-link-check/issues/94
[exclude private domains]: https://github.com/appscodelabs/liche/blob/a5102b0bf90203b467a4f3b4597d22cd83d94f99/url_checker.go
[usable as library]: https://github.com/raviqqe/liche/issues/13
