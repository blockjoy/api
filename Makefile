DATABASE_URL ?= postgres://blockvisor:password@localhost:25432/blockvisor_db
DOCKER_COMPOSE ?= docker-compose --file docker/docker-compose.yaml
DIESEL ?= diesel --config-file blockvisor-api/diesel.toml --database-url ${DATABASE_URL}
TEST_SERVICES ?= postgres emqx

.PHONY: help setup diesel up up-all down reset reset-all test test-out
.DEFAULT_GOAL := help

define fetch_arg
	$(eval target := $(firstword $(MAKECMDGOALS)))
	$(eval arg := $(target)_arg)
	$(eval $(arg) := $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS)))
	$(eval $($(arg))::;@:)
endef

help: ## Print this message and exit.
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "\033[36m%12s\033[0m : %s\n", $$1, $$2}' $(MAKEFILE_LIST)

setup: ## Install the prerequistes for running tests.
	@git submodule update --init --recursive --remote
	@cargo install --force diesel_cli --no-default-features --features postgres

diesel: res := $(call fetch_arg)
diesel: ## Run a diesel cli command.
	@${DIESEL} $($@_arg)

up: ## Start required docker services for integration tests.
	@${DOCKER_COMPOSE} up --detach --wait ${TEST_SERVICES}
	@${DIESEL} migration run

up-all: ## Start all docker services for integration tests and metrics.
	@${DOCKER_COMPOSE} up --detach --wait
	@${DIESEL} migration run

down: ## Stop all running docker services.
	@${DOCKER_COMPOSE} down --volumes
	@rm -rf ./docker/{clickhouse,signoz}/data

reset: down up ## Reset the required docker services.
reset-all: down up-all ## Reset all docker services.

test: res := $(call fetch_arg)
test: ## Run cargo test (usage: `make test <name>`).
	@cargo test --all-features $($@_arg)

test-out: res := $(call fetch_arg)
test-out: ## Run cargo test with stdout (usage: `make test-out <name>`).
	@cargo test --all-features $($@_arg) -- --nocapture
