# Local Codex Session Files as Usage Source

AI Usage will use local Codex session JSONL files under the Codex sessions directory as the first version's authoritative usage source. This keeps the app offline, private, and reproducible, and matches the goal of explaining current local Codex usage rather than importing provider billing data or maintaining a separate usage database.

## Considered Options

- Local Codex session files: chosen because they contain session metadata and token snapshots on disk.
- Provider API or billing export: rejected for the first version because it would add network access and external account coupling.
- App-owned usage database: rejected for the first version because it would duplicate Codex data and introduce drift.
