# Execution
- [x] Read injected context, CodeGraph source and all detector/recovery callers.
- [x] Add a synthetic regression for a 16,766-character bad signature and the full >32 KiB Google message/details echo. The baseline detector and 16 KiB collector cannot satisfy its recovery assertions; an explicit baseline mutation run remains optional verification.
- [x] Implement strict recognition and targeted repair using existing candidate lifecycle.
- [x] Cover direct stream, sync, fixed-target path and diagnostic projection as affected; preserve old tests.
- [x] Run scoped rustfmt/diff checks; provide exact Cargo test filters to root.
- [x] Root runs Docker targeted tests and compilation/lint with serialized shared cache access.
- [x] Independent source check and contract update; final Cargo/Clippy gates passed.
Do not commit, push, start other agents or run competing Cargo/Docker builds. Record bounded findings in research/implementation-receipt.md.
