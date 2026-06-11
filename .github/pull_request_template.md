## Summary

<!-- Brief description of changes -->

## Type
- [ ] Bug fix
- [ ] New feature
- [ ] Documentation
- [ ] CI / infrastructure
- [ ] Refactor

## Testing

<!-- How was this tested? Include test command output -->

```
cargo test
```

## Checklist
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --target wasm32-unknown-unknown -- -D warnings` passes
- [ ] `cargo test` passes
- [ ] `cargo build --release --target wasm32-unknown-unknown` passes
- [ ] Events are emitted for state changes
- [ ] Documentation updated if applicable
