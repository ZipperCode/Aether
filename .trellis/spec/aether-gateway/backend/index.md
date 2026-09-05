# Backend Development Guidelines

> Best practices for backend development in this project.

---

## Overview

This directory contains guidelines for backend development. Fill in each file with your project's specific conventions.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Module organization and file layout | To fill |
| [Database Guidelines](./database-guidelines.md) | ORM patterns, queries, migrations | To fill |
| [Error Handling](./error-handling.md) | Error types, handling strategies | To fill |
| [Quality Guidelines](./quality-guidelines.md) | Code standards, forbidden patterns | To fill |
| [Logging Guidelines](./logging-guidelines.md) | Structured logging, log levels | To fill |
| [Balance-Aware Key Scheduling](../../aether-provider-pool/backend/balance-scheduling-contract.md) | Runtime, sticky, refresh, and cache integration contract | Current |
| [Runtime Key Quota Block](../../aether-provider-pool/backend/runtime-quota-block-contract.md) | Failure classification, effects, admin recovery, and cache integration | Current |
| [Codex HTTP Responses Relay](../../aether-ai-formats/backend/codex-http-responses-contract.md) | Cross-layer create, compact, SSE, auth, and state boundary | Current |
| [Model Capability Test](./model-capability-test-contract.md) | Pinned target/reference execution, random suite, scoring, API, and UI boundary | Current |
| [Provider Model Association Endpoint Evidence](./model-association-endpoint-contract.md) | Default-path discovery, exact association, Endpoint propagation, fallback, and race contract | Current |
| [Authentication Maintenance Memory](./auth-maintenance-memory-contract.md) | Shared auth gate, lightweight Key projection, and lazy candidate body materialization | Current |

---

## How to Fill These Guidelines

For each guideline file:

1. Document your project's **actual conventions** (not ideals)
2. Include **code examples** from your codebase
3. List **forbidden patterns** and why
4. Add **common mistakes** your team has made

The goal is to help AI assistants and new team members understand how YOUR project works.

---

**Language**: All documentation should be written in **English**.
