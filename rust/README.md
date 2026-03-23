# Rust Workspace

This workspace hosts the `ccp` CLI that will eventually replace the Bash-based `cac` tool.

## Testing

```
cd rust
cargo test --test cli_profile
```

The only test today ensures `ccp --help` can be built and run, so this command verifies the workspace and CLI scaffolding are wired.
