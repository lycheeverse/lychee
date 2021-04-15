![lychee](assets/banner.svg)

![Rust](https://github.com/hello-rust/lychee/workflows/Rust/badge.svg)
[![docs.rs](https://docs.rs/lychee/badge.svg)](https://docs.rs/lychee)

⚡ A fast, async, resource-friendly link checker written in Rust.\\
Finds broken hyperlinks and mail addresses inside Markdown, HTML, reStructuredText, or any other text file or website!

Available as a CLI utility and as a GitHub Action: [lycheeverse/lychee-action](https://github.com/lycheeverse/lychee-action).

![Lychee demo](./assets/lychee.gif)

## Features

This comparison is made on a best-effort basis. Please create a PR to fix
outdated information.
use
|                      | lychee  | [awesome_bot] | [muffet] | [broken-link-checker] | [linkinator] | [linkchecker] | [markdown-link-check] | [fink] |
| -------------------- | ------- | ------------- | -------- | --------------------- | ------------ | ------------- | --------------------- | ------ |
| Language             | Rust    | Ruby          | Go       | JS                    | TypeScript   | Python        | JS                    | PHP    |
| Async/Parallel       | ![yes]  | ![yes]        | ![yes]   | ![yes]                | ![yes]       | ![yes]        | ![yes]                | ![yes] |
| JSON output | ![yes] | ![no] | ![yes] | ![yes] | ![yes] | ![maybe]<sup>1</sup> | ![yes] | ![yes] |
| Static binary        | ![yes]  | ![no]         | ![yes]   | ![no]                 | ![no]        | ️ ![no]       | ![no]                 | ![no]  |
| Markdown files       | ![yes]  | ![yes]        | ![no]    | ![no]                 | ![no]        | ![yes]        | ️ ![yes]              | ![no]  |
| HTML files           | ![yes]  | ![no]         | ![no]    | ![yes]                | ![yes]       | ![no]         | ![yes]                 | ![no]  |
| Text files           | ![yes]  | ![no]         | ![no]    | ![no]                 | ![no]        | ![no]         | ![no]                | ![no]  |
| Website support      | ![yes]  | ![no]         | ![yes]   | ![yes]                | ![yes]       | ![yes]        | ![no]                 | ![yes] |
| Chunked encodings    | ![yes]  | ![maybe]      | ![maybe] | ![maybe]              | ![maybe]     | ![no]         | ![yes]                | ![yes] |
| GZIP compression     | ![yes]  | ![maybe]      | ![maybe] | ![yes]                | ![maybe]     | ![yes]        | ![maybe]              | ![no]  |
| Basic Auth           | ![yes]  | ![no]         | ![no]    | ![yes]                | ![no]        | ![yes]        | ![no]                 | ![no]  |
| Custom user agent    | ![yes]  | ![no]         | ![no]    | ![yes]                | ![no]        | ![yes]        | ![no]                 | ![no]  |
| Relative URLs        | ![yes]  | ![yes]        | ![no]    | ![yes]                | ![yes]       | ![yes]        | ![yes]                | ![yes] |
| Skip relative URLs   | ![yes]  | ![no]         | ![no]    | ![maybe]              | ![no]        | ![no]         | ![no]                 | ![no]  |
| Include patterns     | ![yes]️ | ![yes]        | ![no]    | ![yes]                | ![no]        | ![no]         | ![no]                 | ![no]  |
| Exclude patterns     | ![yes]  | ![no]         | ![yes]   | ![yes]                | ![yes]       | ![yes]        | ![yes]                | ![yes] |
| Handle redirects     | ![yes]  | ![yes]        | ![yes]   | ![yes]                | ![yes]       | ![yes]        | ![yes]                | ![yes] |
| Ignore insecure SSL  | ![yes]  | ![yes]        | ![yes]   | ![no]                 | ![no]        | ![yes]        | ![no]                 | ![yes] |
| File globbing        | ![yes]  | ![yes]        | ![no]    | ![no]                 | ![yes]       | ![no]         | ![yes]                | ![no]  |
| Limit scheme         | ![yes]  | ![no]         | ![no]    | ![yes]                | ![no]        | ![yes]        | ![no]                 | ![no]  |
| [Custom headers]     | ![yes]  | ![no]         | ![yes]   | ![no]                 | ![no]        | ![no]         | ![yes]                | ![yes] |
| Summary              | ![yes]  | ![yes]        | ![yes]   | ![maybe]              | ![yes]       | ![yes]        | ![no]                 | ![yes] |
| `HEAD` requests      | ![yes]  | ![yes]        | ![no]    | ![yes]                | ![yes]       | ![yes]        | ![no]                 | ![no]  |
| Colored output       | ![yes]  | ![maybe]      | ![yes]   | ![maybe]              | ![yes]       | ![yes]        | ![no]                 | ![yes] |
| [Filter status code] | ![yes]  | ![yes]        | ![no]    | ![no]                 | ![no]        | ![no]         | ![yes]                | ![no]  |
| Custom timeout       | ![yes]  | ![yes]        | ![yes]   | ![no]                 | ![yes]       | ![yes]        | ![no]                 | ![yes] |
| E-mail links         | ![yes]  | ![no]         | ![no]    | ![no]                 | ![no]        | ![yes]        | ![no]                 | ![no]  |
| Progress bar         | ![yes]  | ![yes]        | ![no]    | ![no]                 | ![no]        | ![yes]        | ![yes]                | ![yes] |
| Retry and backoff    | ![yes]  | ![no]         | ![no]    | ![no]                 | ![yes]       | ![no]         | ![yes]                | ![no]  |
| Skip private domains | ![yes]  | ![no]         | ![no]    | ![no]                 | ![no]        | ![no]         | ![no]                 | ![no]  |
| [Use as library]     | ![yes]  | ![yes]        | ![no]    | ![yes]                | ![yes]       | ![no]         | ![yes]                | ![no]  |
| Quiet mode           | ![yes]  | ![no]         | ![no]    | ![no]                 | ![yes]       | ![yes]        | ![yes]                | ![yes] |
| Config file          | ![yes]  | ![no]         | ![no]    | ![no]                 | ![yes]       | ![yes]        | ![yes]                | ![no]  |
| Recursion | ![no]  | ![no]         | ![no]    | ![yes]                 | ![yes]        | ![yes]         | ![yes]                 | ![no]  |
| Amazing lychee logo  | ![yes]  | ![no]         | ![no]    | ![no]                 | ![no]        | ![no]         | ![no]                 | ![no]  |

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
[use as library]: https://github.com/raviqqe/liche/issues/13

<sup>1</sup> Other machine-readable formats like CSV are supported.

## Contributing to lychee

We'd be thankful for any contribution. \
We try to keep the issue-tracker up-to-date so you can quickly find a task to work on.

Try one of these links to get started:

- [good first issues](https://github.com/lycheeverse/lychee/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)
- [help wanted](https://github.com/lycheeverse/lychee/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22)

## Using the Commandline Client

You can run lychee directly from the commandline.

### Installation

#### Using cargo

```sh
cargo install lychee
```

#### Using the official Docker image

```sh
docker pull lycheeverse/lychee
```

#### Using pre-built binaries

We provide binaries for Linux, macOS, and Windows for every release. \
You can download them from the [releases page](https://github.com/lycheeverse/lychee/releases).

## Commandline usage

Run it inside a repository with a `README.md`:

```sh
lychee
```

You can also specify various types of inputs:

```sh
# check links on a website:
lychee https://endler.dev/

# check links in a remote file:
lychee https://raw.githubusercontent.com/lycheeverse/lychee/master/README.md

# check links in local file(s):
lychee README.md
lychee test.html info.txt

# check links in local files (by shell glob):
lychee ~/projects/*/README.md

# check links in local files (lychee supports advanced globbing and ~ expansion):
lychee "~/projects/big_project/**/README.*"
# ignore case when globbing and check result for each link:
lychee --glob-ignore-case --verbose "~/projects/**/[r]eadme.*"
```

### GitHub token

Optionally, to avoid getting rate-limited while checking GitHub links, you can
set an environment variable with your Github token like so `GITHUB_TOKEN=xxxx`,
or use the `--github-token` CLI option. It can also be set in the config file.

The token can be generated in your
[GitHub account settings page](https://github.com/settings/tokens). A personal
token with no extra permissions is enough to be able to check public repos links.

### Commandline Parameters

There is an extensive list of commandline parameters to customize the behavior,
see below for a full list.

```ignore
USAGE:
    lychee [FLAGS] [OPTIONS] [--] [inputs]...

FLAGS:
    -E, --exclude-all-private    Exclude all private IPs from checking.
                                 Equivalent to `--exclude-private --exclude-link-local --exclude-loopback`
        --exclude-link-local     Exclude link-local IP address range from checking
        --exclude-loopback       Exclude loopback IP address range from checking
        --exclude-mail           Exclude all mail addresses from checking
        --exclude-private        Exclude private IP address ranges from checking
        --glob-ignore-case       Ignore case when expanding filesystem path glob inputs
        --help                   Prints help information
    -i, --insecure               Proceed for server connections considered insecure (invalid TLS)
    -n, --no-progress            Do not show progress bar.
                                 This is recommended for non-interactive shells (e.g. for continuous integration)
        --skip-missing           Skip missing input files (default is to error if they don't exist)
    -V, --version                Prints version information
    -v, --verbose                Verbose program output

OPTIONS:
    -a, --accept <accept>                      Comma-separated list of accepted status codes for valid links
    -b, --base-url <base-url>                  Base URL to check relative URLs
        --basic-auth <basic-auth>              Basic authentication support. E.g. `username:password`
    -c, --config <config-file>                 Configuration file to use [default: ./lychee.toml]
        --exclude <exclude>...                 Exclude URLs from checking (supports regex)
    -f, --format <format>                      Output file format of status report (json, string) [default: string]
        --github-token <github-token>          GitHub API token to use when checking github.com links, to avoid rate
                                               limiting [env: GITHUB_TOKEN=]
    -h, --headers <headers>...                 Custom request headers
        --include <include>...                 URLs to check (supports regex). Has preference over all excludes
        --max-concurrency <max-concurrency>    Maximum number of concurrent network requests [default: 128]
    -m, --max-redirects <max-redirects>        Maximum number of allowed redirects [default: 10]
    -X, --method <method>                      Request method [default: get]
    -o, --output <output>                      Output file of status report
    -s, --scheme <scheme>                      Only test links with the given scheme (e.g. https)
    -T, --threads <threads>                    Number of threads to utilize. Defaults to number of cores available to
                                               the system
    -t, --timeout <timeout>                    Website timeout from connect to response finished [default: 20]
    -u, --user-agent <user-agent>              User agent [default: lychee/0.6.0]

ARGS:
    <inputs>...    The inputs (where to get links to check from). These can be: files (e.g. `README.md`), file globs
                   (e.g. `"~/git/*/README.md"`), remote URLs (e.g. `https://example.org/README.md`) or standard
                   input (`-`). Prefix with `--` to separate inputs from options that allow multiple arguments
                   [default: README.md]
```

### Exit codes

- `0` for success (all links checked successfully or excluded/skipped as configured)
- `1` for missing inputs and any unexpected runtime failures or config errors
- `2` for link check failures (if any non-excluded link failed the check)

## Library usage

You can use lychee as a library for your own projects.
Here is a "hello world" example:

```rust
use lychee_lib::Result;

#[tokio::main]
async fn main() -> Result<()> {
  let response = lychee_lib::check("https://github.com/lycheeverse/lychee").await?;
  println!("{}", response);
  Ok(())
}
```

This is equivalent to the following snippet, in which we build our own client:

```rust
use lychee_lib::{ClientBuilder, Result, Status};

#[tokio::main]
async fn main() -> Result<()> {
  let client = ClientBuilder::default().client()?;
  let response = client.check("https://github.com/lycheeverse/lychee").await?;
  assert!(response.status().is_success());
  Ok(())
}
```

The client builder is very customizable:

```rust, ignore
let client = lychee_lib::ClientBuilder::builder()
    .includes(includes)
    .excludes(excludes)
    .max_redirects(cfg.max_redirects)
    .user_agent(cfg.user_agent)
    .allow_insecure(cfg.insecure)
    .custom_headers(headers)
    .method(method)
    .timeout(timeout)
    .verbose(cfg.verbose)
    .github_token(cfg.github_token)
    .scheme(cfg.scheme)
    .accepted(accepted)
    .build()
    .client()?;
```

All options that you set will be used for all link checks.
See the [builder documentation](https://docs.rs/lychee/latest/lychee/struct.ClientBuilder.html) for all options.

## GitHub Action usage

A GitHub Action that uses lychee is available as a separate repository: [lycheeverse/lychee-action](https://github.com/lycheeverse/lychee-action)
which includes usage instructions.

## Troubleshooting and workarounds

We collect a list of common workarounds for various websites in our [troubleshooting guide](./TROUBLESHOOTING.md).

## Users

- https://github.com/pawroman/links
- https://github.com/analysis-tools-dev/static-analysis
- https://github.com/analysis-tools-dev/dynamic-analysis
- https://github.com/mre/idiomatic-rust
- https://github.com/lycheeverse/lychee (yes, the lychee docs are checked with lychee 🤯)

If you are using lychee for your project, we'd be delighted to hear about it.

## License

lychee is licensed under either of

- Apache License, Version 2.0, (LICENSE-APACHE or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license (LICENSE-MIT or http://opensource.org/licenses/MIT)

at your option.

## Credits

The first prototype of lychee was built in [episode 10 of Hello
Rust](https://hello-rust.show/10/). Thanks to all Github- and Patreon sponsors
for supporting the development since the beginning. Also, thanks to all the
great contributors who have since made this project more mature.
