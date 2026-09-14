# Release CI recovery validation

- Original release candidate: 238c09382a44aebd4c3c4c83f5a3ff094b83cf6c.
- GitHub Rust CI: https://github.com/ZipperCode/Aether/actions/runs/34864784095 . Gateway and every other substantive job passed; Workspace Rest ran 3821 tests with 3820 passed, 1 failed, 30 skipped. The failed lifecycle pending assertion is documented in the PRD; failed Test/check jobs are aggregate consequences.
- Local validation uses pinned Windows Rust 1.95 with target on C drive, dev/test debug=0, incremental=0 and RUST_MIN_STACK=16777216. D drive had only about 40 MiB available; no Docker-volume expansion or user-file cleanup was attempted.
- First regression compilation exposed unpinned async futures at two poll sites (E0277). Independent review fixed both with std::pin::pin!.
- Fixed-source five exact tests passed: new drain regression, first-byte retry, permanent Redis/database failure, slow-database recovery and terminal enqueue burst. Result: 5 passed, 0 failed, 362 filtered, 1.11s; test rebuild 23.07s.
- Temporarily removing only the two added drain conditions deterministically failed the new regression: 0 passed, 1 failed, 366 filtered, 0.00s. Failure: drain must wait for lifecycle submission completion. See red-output.txt. The original bytes were restored in finally.
- Reviewed/restored source SHA-256: 00C67F54F6F01E6033D32874698F87B90213697E354A7206942A9A1E1A63052C. All changes are under cfg(test); production queue logic, timeouts, capacities and assertions are unchanged.
- Independent review, pinned Rust formatting and git diff checks passed. Coordinator will merge without changing the 33 original WIP files and require a fresh full exact-SHA GitHub CI gate before v0.7.34.
