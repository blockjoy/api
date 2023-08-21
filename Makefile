DATABASE_URL ?= postgres://blockvisor:password@localhost:25432/blockvisor_db

.PHONY: help setup start stop test test-nc
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
	@cargo install diesel_cli --no-default-features --features postgres

start: ## Start all docker services for integration tests.
	@docker-compose up --detach --wait
	@DATABASE_URL=${DATABASE_URL} diesel migration run

stop: ## Stop all running docker services.
	@docker-compose down --volumes
	@rm -rf ./docker/{clickhouse,signoz}/data

test: res := $(call fetch_arg)
test: ## Run cargo test (usage: `make test <name>`).
	@cargo test --all-features $($@_arg)

test-out: res := $(call fetch_arg)
test-out: ## Run cargo test with stdout (usage: `make test-out <name>`).
	@cargo test --all-features $($@_arg) -- --nocapture
