![lychee](assets/banner.png)

![Rust](https://github.com/hello-rust/lychee/workflows/Rust/badge.svg)

...because who says I can't write yet another link checker.

## What?

This thing was created from [Hello Rust Episode
10](https://hello-rust.show/10/). It's a link checker that treats Github links
specially by using a `GITHUB_TOKEN` to avoid getting blocked by the bot
protection.

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
