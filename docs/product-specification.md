# AI Usage Product Specification

AI Usage is a Rust terminal application for understanding the United States dollar cost of local AI coding agent usage.

## First Version Scope

- Track Codex CLI, Pi Coding Agent, and OpenCode usage.
- Read local Codex session JSONL files from the Codex sessions directory, Pi session JSONL files from the Pi sessions directory, and OpenCode session files from the OpenCode data directory.
- Support a custom source path with `--usage-source`.
- Avoid persistent app configuration and avoid an app-owned usage database.
- Work fully offline with a bundled local price catalog.
- Provide both an interactive terminal interface and non-interactive JSON output.

## Source Semantics

- Treat local AI coding agent session files as the authoritative usage source.
- Treat each source-level session as one AI Coding Agent Session after source-specific parsing, and preserve which AI coding agent produced it.
- Preserve source-recorded session names when available, without inferring names from prompt content.
- Use the session start time for daily, weekly, monthly, and all-time grouping.
- Use the final usable Codex token snapshot for Codex Session totals; sum Pi and OpenCode assistant-message usage entries before they become AI Coding Agent Sessions.
- Include active sessions with the latest available token snapshot and a visible active marker.
- Skip unknown or malformed records, preserve partial results, and show data quality notices.
- Never infer prompt content or estimate missing token counts from message text.

## Token Semantics

- Preserve input tokens, non-cached input tokens, cache-read tokens, cache-write tokens, output tokens, reasoning output tokens, and total tokens when available.
- Do not double count cache-read tokens as separate from input tokens.
- Derive non-cached input tokens from input tokens minus cache-read tokens when both values are known.
- Preserve reasoning output tokens as detail and price them as output tokens unless a price schedule distinguishes them.

## Cost Semantics

- Calculate historical cost from the price schedule effective at the session start time.
- Use United States dollars only.
- Keep calculation precision through aggregation and round only display values.
- Show a known partial cost with an incomplete marker when a period includes unpriced or incomplete usage.
- Treat missing price schedules as unpriced usage, not zero-cost usage.

## Terminal Interface

- Show daily, weekly, monthly, and all-time headline totals together.
- Lead with cost and show token counts as supporting detail.
- Use theme-aware terminal colors with a Matte Box-inspired fallback palette.
- Keep the visual style restrained: compact spacing, minimal borders, and markers that work without color.
- Show each finite headline period's cost change from the previous matching period as compact supporting context.
- Show selected-period session detail sorted newest first.
- Show the price-per-million-token schedule summary used for each session.
- Show all-time detail grouped by month first.
- Show the AI coding agent label and project names in compact session lists, with full paths only in JSON output.
- Exclude prompt and conversation content from every terminal view.

## Keyboard Model

- Use arrow keys and `h`, `j`, `k`, `l` for navigation.
- Use `r` to reload the current source from disk.
- Use `q` and `Esc` to quit.

## JSON Output

- Enable non-interactive JSON with `--json`.
- Include an output schema version.
- Include source paths by default for audit and testing.
- Include source-recorded session names in session detail when available.
- Support `--redact-paths` to replace home-directory-sensitive path prefixes.
- Represent the same derived usage information as the terminal summary.

## Out Of Scope

- Provider billing APIs.
- Live pricing fetches or update checks.
- CSV, PDF, or report-file export.
- Prompt or conversation review.
- Persistent preferences.
- Converted local currencies.
