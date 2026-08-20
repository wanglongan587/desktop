# Domain Docs

This repository uses a single-context domain documentation layout.

## Before exploring

Read these sources when they exist and are relevant:

- `CONTEXT.md` at the repository root.
- ADRs under `docs/adr/`.

If either source does not exist, proceed silently. Do not create placeholder domain documentation merely to satisfy this layout. Domain-modeling workflows create these files when actual terminology or architectural decisions need to be recorded.

## Layout

```text
/
├── CONTEXT.md
├── docs/
│   └── adr/
└── crates/
```

The pnpm packages and Rust crates are technical modules within one Ora product context. They do not automatically constitute separate domain contexts.

## Vocabulary

Use terms as defined in the root `CONTEXT.md` when it exists. Do not replace defined terms with near-synonyms.

If a needed concept is absent, first determine whether the codebase already uses another term. Record a domain gap only when the concept is genuinely missing.

## ADR conflicts

If proposed work contradicts an existing ADR, surface the conflict explicitly instead of silently overriding it.
