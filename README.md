# BlockJoy Api Server

## Database
Copy `.env-example` to `.env` and properly configure the `DATABASE_URL`.

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

## Update protos
!! Important, before updating the protos, make sure that there are no changes in the local repo.

```bash
git rm -r proto
git commit -m "update protos"
git subtree add --prefix proto https://github.com/blockjoy/api-proto HEAD --squash
```
