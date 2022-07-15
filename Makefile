# Needed SHELL since I'm using zsh
SHELL := /bin/bash
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

.PHONY: screencast
screencast: ## Create a screencast for the docs
	svg-term --command="bash assets/screencast.sh" --out assets/screencast.svg --padding=10 --window --width 100