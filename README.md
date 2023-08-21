# Blockvisor API

To see the list of available commands, run `make help` (or just `make` as it is
the default action).

Before running any other steps, run `make setup` to ensure all prerequisites are
in place.

## Running integration tests

First run `make start` to start the required docker-compose services.

Then run `make test` to run the full integration testing suite. If you want to
run a specific test, pass it as an argument (e.g. `make test my_test`).

When you are finished, run `make stop` to tear down all the docker-compose
services again.

## Updating protos

All protobuf schema updates are made with a Pull Request to the
[api-protos](https://github.com/blockjoy/api-proto) repo. Once merged there, the
submodule in this repo can be updated with:

``` sh
cd proto
git checkout main
git pull
```

For testing changes in this repo against an in-progress `api-proto` branch you
can run `git checkout [branch]`, but should switch the submodule back to `main`
before merging changes to this repo.
