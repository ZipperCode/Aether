# Execution

1. Review four independent slices; confirm actionable findings from source and behavioral reproduction.
2. Fix confirmed issues with scoped ownership. Run tests only after edits settle; use Docker PostgreSQL wrappers if required. Smoke affected runtime/UI paths.
3. Record findings and verification; commit scoped task changes. No empty commit if no fixes.
4. Push origin master, identify Rust CI run by exact SHA/event/branch, watch to completion. Fix any failures before advancing.
5. Confirm no remote tag collision, create annotated v0.7.42 with message `发布 v0.7.42` on CI-proven SHA, push only that tag.
6. Observe Release Aether; verify release assets/status. Record receipt and task completion without moving tag.

Commands follow repository convention: cargo fmt --all --check; targeted cargo test/clippy; frontend npm run type-check/test:run/build (never npm run lint --fix). No manifest version bump; tag determines release identity; Pages remains disabled.
