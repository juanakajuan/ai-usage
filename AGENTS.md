## Naming Policy

Do not use abbreviations, acronyms, or shortened names in generated code. Use complete, descriptive words for all identifiers.

- Prefer clarity over brevity.
- Names must clearly reflect purpose and behavior.

**Examples:**

- `numUsers` -> `numberOfUsers`
- `cfg` -> `configuration`
- `btnClick` -> `buttonClickHandler`
- `usrData` -> `userData`

Applies to variables, functions, classes, and files.

> Use full words even if names become longer, unless readability is significantly reduced, or if it is part of the language's naming standard.

## Type Safety

Enforce strict typing in all supported languages (e.g. TypeScript, Rust).

- Use strict compiler settings (e.g. `strict: true`)
- Avoid `any`, unchecked casts, and type suppression
- Define explicit types for public APIs and data structures
- Validate external data at boundaries

Weakening type safety is considered a defect.

## Agent skills

### Issue tracker

Issues are tracked in GitHub Issues for `juanakajuan/ai-usage` using the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The repo uses the default five-label triage vocabulary. See `docs/agents/triage-labels.md`.

### Domain docs

This is a single-context repo with root `CONTEXT.md` and `docs/adr/`. See `docs/agents/domain.md`.

## Other

- If a repository has one or more `CONTEXT.md` files, read them before making changes.
- Add proper documentation according to each language's standard conventions, such as TSDoc for TypeScript or XML documentation comments for C#. Document purpose, parameters, return values, and any non-obvious logic for new code and existing code the agent interacts with or modifies.
