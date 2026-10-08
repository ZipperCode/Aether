# Integration design

Use a no-commit Git merge pinned to 911c7f887. Parent owns Git index and merge state. Delegate non-overlapping conflict domains after Git identifies actual conflicts; agents edit assigned paths only and do not run checks mid-flight. Preserve local behavior where upstream is additive; migrate callers when upstream intentionally replaces APIs. Decisions that change user behavior without a clear combined implementation require user input.

Validation covers Rust workspace, frontend type-check/build/existing suites, isolated gateway runtime and actual UI. PostgreSQL migrations require an isolated database and must not touch deployed state. Record unavailable prerequisites explicitly. Keep Pages disabled and fork deployment identity unchanged.
