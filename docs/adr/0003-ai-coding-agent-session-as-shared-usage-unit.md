# AI Coding Agent Session as Shared Usage Unit

AI Usage will use AI Coding Agent Session as the shared usage unit after source-specific parsing. Codex Session remains the source-specific concept for Codex records and Final Token Snapshot selection; Pi Usage and OpenCode Usage are translated from their own assistant-message usage entries into AI Coding Agent Sessions before Historical Cost, Reporting Periods, Session Detail, JSON Output, and terminal presentation consume them.

This keeps source-specific rules local to source parsing while giving downstream modules one domain concept for grouping, pricing, and display.

This decision does not replace ADR-0002's Codex-specific Usage Source choice; it defines what downstream modules consume after Codex Usage, Pi Usage, and OpenCode Usage are parsed.

## Considered Options

- AI Coding Agent Session as the shared usage unit: chosen because the first version tracks Codex Usage, Pi Usage, and OpenCode Usage, and downstream behavior is shared after source-specific parsing.
- Codex Session as the shared usage unit: rejected because it makes non-Codex usage carry Codex-specific language and token snapshot invariants.
- Individual model request as the shared usage unit: rejected because AI Usage groups and displays usage by session first, with lower-level records as supporting detail.
