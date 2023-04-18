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
```bash
git rm -r proto
git commit -m "fix: delete old protos"
git push origin <branch>
git subtree [add|pull] --prefix proto https://github.com/blockjoy/api-proto <version tag> --squash
```
