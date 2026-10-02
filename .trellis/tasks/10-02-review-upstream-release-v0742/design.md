# Design

Read-only review slices: protocol/stream, routing/quota, runtime/UI, release workflow. Parent integrates verified fixes and owns all Git mutations. Review against pre-integration 47681f9ec and both merge parents to distinguish introduced defects from historical limitations.

Release gate: local verification -> commit -> ordinary push origin master -> exact pushed SHA GitHub Rust CI successful -> immutable next version tag on that SHA -> observe tag-triggered release workflow. No Pages enablement or unrelated deployment changes. Preserve commits; failures fixed through new commits and repeat CI gate.
