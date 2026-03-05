# Security Audit Design

Date: 2026-03-05

## Overview
This design defines a staged security audit for the Aether repository (backend, frontend, proxy, deployment artifacts). The system is publicly accessible, and the goal is to identify and remediate vulnerabilities with minimal behavioral changes, prioritizing high-risk issues.

## Scope
In scope:
- Backend service (`src`)
- Frontend console (`frontend`)
- Proxy node (`aether-proxy`)
- Dockerfiles, compose files, deploy scripts, and configuration examples
- Tooling scripts (e.g., key generation)

Out of scope:
- Active exploitation against production
- Access to real credentials or user data

## Assumptions
- Service is exposed to the public internet.
- The audit is code/configuration focused (static review + targeted verification), not a live penetration test.
- Fixes should minimize breaking changes unless risk severity requires stronger enforcement.

## Phased Audit Plan
Phase 1: Attack surface and entrypoint mapping
- Enumerate public endpoints, admin surfaces, proxy exposure, and config defaults.

Phase 2: Authentication and authorization
- JWT usage, admin auth, API key issuance/validation, role boundaries, multi-tenant isolation.

Phase 3: Input and protocol safety
- Validation, injection risks, SSRF, request smuggling, proxy misconfig, unsafe deserialization.

Phase 4: Secrets and cryptography
- Key generation, storage, encryption at rest, log redaction, key rotation practices.

Phase 5: Dependencies and supply chain
- Python/Node dependencies, container base images, build scripts, known CVEs.

Phase 6: Deployment and runtime hardening
- Docker/compose configs, CORS, headers, rate limits, network policies, least-privilege defaults.

Each phase outputs findings with severity, exploit path, and remediation patches. Work proceeds to the next phase only after approval.

## Deliverables
- Phase-by-phase vulnerability list with impact and reproduction guidance
- Remediation plan with code/config changes
- Patch set applied to repository with clear references
- Verification notes (tests or checks run)

## Risks and Trade-offs
- Some mitigations may tighten access controls and could affect compatibility; changes will be proposed with rationale.
- Full dynamic testing is not included; if needed, it can be added later as a separate phase.
