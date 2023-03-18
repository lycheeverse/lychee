# Contributing to lychee

## Getting Started

Lychee is written in Rust. Install [rust-up](https://rustup.rs/) to get started.
Begin by making sure the following commands succeed without errors.

```sh
cargo test # runs tests
cargo clippy # lints code
```

## Picking an Issue

We try to keep the issue-tracker up-to-date so you can quickly find a task to work on.

Try one of these links to get started:

- [good first issues](https://github.com/lycheeverse/lychee/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)
- [help wanted](https://github.com/lycheeverse/lychee/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22)

If you would like to contribute a new feature, the best way to get started is to
ask for some early feedback by creating an issue yourself and asking for feedback.

## Development Workflow

1. Create a new development branch for your feature or bugfix.
2. Make your changes and commit them.
3. Run `cargo test` and `cargo clippy` to make sure your changes don't break anything.
   We provide a few `make` targets to make this easier:
   - `make lint` runs `cargo clippy` on all crates
   - `make help` lists all available targets
4. Push your changes to your fork and create a pull request.

## Thanks!

No matter how small, we appreciate every contribution. You're awesome!
