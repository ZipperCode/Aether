# Implementation

1. Resolve reusable active execution evidence and cleanup/test seams. Parent owns data cleanup and candidate persistence; diagnostics worker owns execution_runtime transport/stream files.
2. Remove streaming-as-success recovery and protect active streams without an elapsed-time cap. Align terminal candidate updates with real request failures and preserve late nonterminal suppression. Add behavior regressions in the owning data/runtime suites.
3. Preserve safe body-read error classification; add bounded body-free stream event/progress diagnostics across direct stream consumers. Add focused observer and loopback tests.
4. Integrate once and run focused tests with Rust 1.95.0, `RUST_MIN_STACK=16777216`, and repository CI debug/incremental settings. Use a disposable local PostgreSQL container only because changed cleanup SQL requires a real database behavior check. Do not use other projects databases or production.
5. Run relevant architecture tests, `cargo fmt --all --check`, and `git diff --check`; inspect all changes. Commit/push isolated verified files under current branch policy. Do not bypass hooks or repair unrelated failures.

The user has approved this scope and explicitly excluded a total-duration cap. No additional approval is needed for the scoped local implementation.
