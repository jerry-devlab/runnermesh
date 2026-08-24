# Contributing

RunnerMesh is in early development. Small, focused contributions that preserve the documented architecture are preferred.

## Before opening a pull request

1. Keep the scope narrow and explain any product-contract change.
2. Run `cargo check` and `cargo test` when a Rust toolchain is available.
3. Keep documentation accurate about what is planned versus implemented.
4. Do not add private dogfood information, credentials, machine details, or personal topology to code, fixtures, examples, or documentation.

## CI expectations

Public pull requests are validated by GitHub-hosted CI. Contributors do not need to provide, configure, or expose self-hosted runners.

## Design contributions

When proposing behavior, preserve human-first admission, CI-platform-native boundaries, explicit trust boundaries, and stable machine-readable contracts once introduced. Discuss broad new authority or platform scope before implementing it.
