# FRONTEND KNOWLEDGE BASE

## OVERVIEW

Vue 3/Vite/TypeScript application for public, user, and admin surfaces; route views orchestrate reusable domain features over a centralized authenticated API client.

## STRUCTURE

```text
src/
├── api/          # Authenticated Axios client and typed domain endpoints
├── features/     # Reusable domain UI and logic
├── views/        # Route-level public/user/admin orchestration
├── router/       # Route tables and auth/admin/module guards
├── stores/       # Pinia identity, modules, users, proxy nodes
├── components/   # Cross-domain UI/layout/chart primitives
├── composables/  # Generic reusable behavior
└── i18n/         # Runtime i18n and legacy template transform
```

## WHERE TO LOOK

| Task | Location | Notes |
|---|---|---|
| App bootstrap/global failures | `src/main.ts`, `src/App.vue` | Installs Pinia, i18n, router, global overlays |
| Route/permission behavior | `src/router/routes/`, `src/router/guards/` | Protected by default; admin and module meta are distinct |
| Authentication/retry/device ID | `src/api/client.ts` | Do not reproduce in components or stores |
| Typed backend contracts | `src/api/endpoints/types/` | Update alongside payload changes |
| Reusable domain work | `src/features/` | Providers, pool, usage, users, wallet, routing |
| Route pages | `src/views/{public,user,admin,shared}/` | Composition, not reusable domain ownership |
| Design system | `DESIGN_SYSTEM.md`, `src/style.css`, `src/components/ui/` | Paper/book theme, responsive, WCAG 2.1 |

## CONVENTIONS

- Use Vue Composition API with `<script setup>` and PascalCase component tags.
- Declare props before emits. Use strict types and `@/*` imports for `src/*`.
- Call domain API modules; preserve `api/client.ts` bearer, cookie, refresh, device-id, demo, and 401 behavior.
- Admin module configuration uses availability; user access uses active state. Do not collapse the distinction.
- Put reusable domain UI in `features`; keep views focused on route orchestration.
- Co-locate Vitest `*.spec.ts` tests; run a targeted spec before the full suite.

## ANTI-PATTERNS

- Never edit `node_modules/`, `dist/`, or `package-lock.json` by hand.
- Do not use raw Axios/fetch from UI code when an API module exists.
- Do not bypass router guards or default protected-route semantics.
- Do not remove the legacy i18n transform from Vite or Vitest configuration.
- Do not use production `console`/`debugger`; route diagnostics through `src/utils/logger.ts`.
- Do not run `npm run lint` expecting a read-only check; it applies fixes.

## COMMANDS

```bash
npm ci
npm run dev
npm run type-check
npm run test:run
npm run build
npm run build:with-typecheck
npx vitest run path/to/example.spec.ts
```

`npm run build` runs Vite only; pair it with `type-check` when validating TypeScript.
