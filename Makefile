# Needed SHELL since I'm using zsh
SHELL := /usr/bin/env bash
IMAGE_NAME := "lycheeverse/lychee"

.PHONY: help
help: ## This help message
	@echo -e "$$(grep -hE '^\S+:.*##' $(MAKEFILE_LIST) | sed -e 's/:.*##\s*/:/' -e 's/^\(.\+\):\(.*\)/\\x1b[36m\1\\x1b[m:\2/' | column -c2 -t -s :)"

.PHONY: docker-build
docker-build: ## Build Docker image
	docker build -t $(IMAGE_NAME) .

.PHONY: docker-run
docker-run: ## Run Docker image
	docker run $(IMAGE_NAME)

.PHONY: docker-push
docker-push: ## Push image to Docker Hub
	docker push $(IMAGE_NAME)

.PHONY: clean
clean: ## Clean up build artifacts
	cargo clean

.PHONY: build
build: ## Build Rust code locally
	cargo build

.PHONY: install
install: ## Install project locally
	cargo install --path lychee-bin --locked

.PHONY: run
run: ## Run project locally
	cargo run

.PHONY: docs
docs: ## Generate and show documentation
	cargo doc --open

.PHONY: lint
lint: ## Run linter
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings

.PHONY: lint-fix
lint-fix: ## Fix linter issues
	cargo fmt --all
	cargo clippy --all-targets --all-features --fix --allow-dirty --allow-staged

.PHONY: test
test: ## Run tests
	cargo nextest run --all-targets --all-features
	cargo nextest run
	cargo test --doc

.PHONY: bench
bench: ## Run benchmarks
	cargo bench

.PHONY: doc
doc: ## Open documentation
	cargo doc --open

.PHONY: screencast
screencast: ## Create a screencast for the docs
	termsvg rec --command=assets/screencast.sh recording.asc
	termsvg export --minify recording.asc --output=assets/screencast.svg
	rm recording.asc

.PHONY: verify
verify: ## Verify the MSRV
	cargo msrv --path lychee-lib verify
	cargo msrv --path lychee-bin verify
