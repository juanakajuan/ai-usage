# Local Codex Session Files as Usage Source

Related: [ADR-0003](0003-ai-coding-agent-session-as-shared-usage-unit.md) defines the shared usage unit consumed after source-specific parsing.

AI Usage will use local Codex session JSONL files under the Codex sessions directory as the authoritative Usage Source for Codex Usage. This keeps the Codex path offline, private, and reproducible, and matches the goal of explaining current local usage rather than importing provider billing data or maintaining a separate usage database.

## Considered Options

- Local Codex session files: chosen because they contain session metadata and token snapshots on disk.
- Provider API or billing export: rejected for the first version because it would add network access and external account coupling.
- App-owned usage database: rejected for the first version because it would duplicate Codex data and introduce drift.
