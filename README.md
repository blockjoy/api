# BlockJoy Api Server

### Install Diesel CLI
```bash
cargo install diesel_cli --no-default-features --features postgres
```

Run Setup:
```bash
diesel setup
```

Run Migrations:
```bash
diesel migration run
```

Revert Migration:
```bash
diesel migration revert
```

## Testing
To run:
```bash
cargo test
```

### Run specific test
If you want to use test name pattern matching of `cargo test` but still having the environment set by `Makefile` you could run the following command:

```bash
make test-with test=my_test_function
```


## Update protos
!! Important, before updating the protos, make sure that there are no changes in the local repo.

```bash
git rm -r proto
git commit -m "update protos"
git subtree add --prefix proto https://github.com/blockjoy/api-proto HEAD --squash
```
