![lychee](assets/banner.png)

![Rust](https://github.com/hello-rust/lychee/workflows/Rust/badge.svg)

A fast, async, resource-friendly link checker written in Rust.
For GitHub links, it can optionally use a `GITHUB_TOKEN` to avoid getting blocked by the rate
limiter.

![Lychee demo](./assets/lychee.gif)

## Features

This comparison is made on a best-effort basis. Please create a PR to fix outdated information.

|                      | lychee  | [awesome_bot] | [muffet]   | [broken-link-checker] | [linkinator] | [linkchecker] | [markdown-link-check] | [fink]   |
| -------------------- | ------- | ----------- | -------- | ------------------- | ---------- | ----------- | ------------------- | ------ |
| Language             | Rust    | Ruby        | Go       | JS                  | TypeScript | Python      | JS                  | PHP    |
| Async/Parallel       | ![yes]  | ![yes]      | ![yes]   | ![yes]              | ![yes]     | ![yes]      | ![yes]              | ![yes] |
| Static binary        | ![yes]  | ![no]       | ![yes]   | ![no]               | ![no]      | ️ ![no]      | ![no]               | ![no]  |
| Markdown files       | ![yes]  | ![yes]      | ![no]    | ![no]               | ![no]      | ![no]       | ️ ![yes]             | ![no]  |
| HTML files           | ![yes]  | ![no]       | ![no]    | ![yes]              | ![yes]     | ![no]       | ![no]               | ![no]  |
| Text files           | ![yes]  | ![no]       | ![no]    | ![no]               | ![no]      | ![no]       | ![no]               | ![no]  |
| Website support      | ![yes]  | ![no]       | ![yes]   | ![yes]              | ![yes]     | ![yes]      | ![no]               | ![yes] |
| Chunked encodings    | ![yes]  | ![maybe]    | ![maybe] | ![maybe]            | ![maybe]   | ![no]       | ![yes]              | ![yes] |
| GZIP compression     | ![yes]  | ![maybe]    | ![maybe] | ![yes]              | ![maybe]   | ![yes]      | ![maybe]            | ![no]  |
| Basic Auth           | ![yes]  | ![no]       | ![no]    | ![yes]              | ![no]      | ![yes]      | ![no]               | ![no]  |
| Custom user agent    | ![yes]  | ![no]       | ![no]    | ![yes]              | ![no]      | ![yes]      | ![no]               | ![no]  |
| Relative URLs        | ![yes]  | ![yes]      | ![no]    | ![yes]              | ![yes]     | ![yes]      | ![yes]              | ![yes] |
| Skip relative URLs   | ![yes]  | ![no]       | ![no]    | ![maybe]            | ![no]      | ![no]       | ![no]               | ![no]  |
| Include patterns     | ![yes]️  | ![yes]      | ![no]    | ![yes]              | ![no]      | ![no]       | ![no]               | ![no]  |
| Exclude patterns     | ![yes]  | ![no]       | ![yes]   | ![yes]              | ![yes]     | ![yes]      | ![yes]              | ![yes] |
| Handle redirects     | ![yes]  | ![yes]      | ![yes]   | ![yes]              | ![yes]     | ![yes]      | ![yes]              | ![yes] |
| Ignore insecure SSL  | ![yes]  | ![yes]      | ![yes]   | ![no]               | ![no]      | ![yes]      | ![no]               | ![yes] |
| File globbing        | ![yes]  | ![yes]      | ![no]    | ![no]               | ![yes]     | ![no]       | ![yes]              | ![no]  |
| Limit scheme         | ![yes]  | ![no]       | ![no]    | ![yes]              | ![no]      | ![yes]      | ![no]               | ![no]  |
| [Custom headers]     | ![yes]  | ![no]       | ![yes]   | ![no]               | ![no]      | ![no]       | ![yes]              | ![yes] |
| Summary              | ![yes]  | ![yes]      | ![yes]   | ![maybe]            | ![yes]     | ![yes]      | ![no]               | ![yes] |
| `HEAD` requests      | ![yes]  | ![yes]      | ![no]    | ![yes]              | ![yes]     | ![yes]      | ![no]               | ![no]  |
| Colored output       | ![yes]  | ![maybe]    | ![yes]   | ![maybe]            | ![yes]     | ![yes]      | ![no]               | ![yes] |
| [Filter status code] | ![yes]  | ![yes]      | ![no]    | ![no]               | ![no]      | ![no]       | ![yes]              | ![no]  |
| Custom timeout       | ![yes]  | ![yes]      | ![yes]   | ![no]               | ![yes]     | ![yes]      | ![no]               | ![yes] |
| E-mail links         | ![yes]  | ![no]       | ![no]    | ![no]               | ![no]      | ![yes]      | ![no]               | ![no]  |
| Progress bar         | ![yes]  | ![yes]      | ![no]    | ![no]               | ![no]      | ![yes]      | ![yes]              | ![yes] |
| Retry and backoff    | ![yes]  | ![no]       | ![no]    | ![no]               | ![yes]     | ![no]       | ![yes]              | ![no]  |
| Skip private domains | ![yes]  | ![no]       | ![no]    | ![no]               | ![no]      | ![no]       | ![no]               | ![no]  |
| [Use as lib]         | ![no]   | ![yes]      | ![no]    | ![yes]              | ![yes]     | ![no]       | ![yes]              | ![no]  |
| Quiet mode           | ![yes]  | ![no]       | ![no]    | ![no]               | ![yes]     | ![yes]      | ![yes]              | ![yes] |
| Amazing lychee logo  | ![yes]  | ![no]       | ![no]    | ![no]               | ![no]      | ![no]       | ![no]               | ![no]  |
| Config file          | ![yes]  | ![no]       | ![no]    | ![no]               | ![yes]     | ![yes]      | ![yes]              | ![no]  |

[awesome_bot]: https://github.com/dkhamsing/awesome_bot
[muffet]: https://github.com/raviqqe/muffet
[broken-link-checker]: https://github.com/stevenvachon/broken-link-checker
[linkinator]: https://github.com/JustinBeckwith/linkinator
[linkchecker]: https://github.com/linkchecker/linkchecker
[markdown-link-check]: https://github.com/tcort/markdown-link-check
[fink]: https://github.com/dantleech/fink
[yes]: ./assets/yes.svg
[no]: ./assets/no.svg
[maybe]: ./assets/maybe.svg
[custom headers]: https://github.com/rust-lang/crates.io/issues/788
[filter status code]: https://github.com/tcort/markdown-link-check/issues/94
[skip private domains]: https://github.com/appscodelabs/liche/blob/a5102b0bf90203b467a4f3b4597d22cd83d94f99/url_checker.go
[use as lib]: https://github.com/raviqqe/liche/issues/13

## Contributing to lychee

We'd be thankful for any contribution.
We try keep the issue-tracker up-to-date so you can quickly find a task to work on.

Try one of these links to get started:
* [good first issues](https://github.com/lycheeverse/lychee/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)
* [help wanted](https://github.com/lycheeverse/lychee/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22)

## Installation

```
cargo install lychee
```

## Usage

Run it inside a repository with a `README.md`, or specify a file with

```
lychee <yourfile>
```

Optional (to avoid getting rate-limited): set an environment variable with your Github token
like so `GITHUB_TOKEN=xxxx`, or use the `--github-token` CLI option. It can also be set in the
config file.

### CLI exit codes

- `0` for success (all links checked successfully or excluded/skipped as configured)
- `1` for any unexpected runtime failures or config errors
- `2` for link check failures (if any non-excluded link failed the check)

## Troubleshooting and workarounds

We collect a list of common workarounds for various websites in our [troubleshooting guide](./TROUBLESHOOTING.md).

## Users

- https://github.com/analysis-tools-dev/static-analysis (soon)
- https://github.com/mre/idiomatic-rust (soon)

## Credits

The first prototype of lychee was built in [episode 10 of Hello Rust](https://hello-rust.show/10/).
Thanks to all Github- and Patreon sponsors for supporting the development since the beginning.
Also, thanks to all the great contributors who have since made this project more mature.
