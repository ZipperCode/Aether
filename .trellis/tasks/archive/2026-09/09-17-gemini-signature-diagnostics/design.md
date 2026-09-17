# Integration design
The child workstreams own disjoint product files. R1 owns provider transport, gateway recovery and affected candidate diagnostic projection. R2 owns usage queue/runtime, PostgreSQL usage-ID projection, the upstream frontend regression and audit note.
The parent owns shared planning, evidence, build scheduling, integration review and final commits. Cargo verification is serialized against existing Docker caches. Synthetic fixtures replace production payloads.
