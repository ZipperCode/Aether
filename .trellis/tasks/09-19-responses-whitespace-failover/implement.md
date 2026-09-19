# Execution

- [x] Diagnose live timing, routing configuration, adjacent upstream preambles, and current source.
- [x] Review the scoped requirements and design under the user's explicit repair request.
- [x] Add a regression and demonstrate failure with unchanged production logic.
- [x] Implement the minimal shared precommit fix and run directly related gateway tests.
- [x] Run independent Trellis review plus affected formatting and Clippy checks.
- [ ] Update the existing stream lifecycle spec, record evidence, commit locally, archive and journal.

Use the existing Docker Rust builder and caches for Cargo execution; the parent coordinates compilation so two agents do not contend for the same target. Use explicit response collection limits in all fixtures. Do not call production models for tests.
