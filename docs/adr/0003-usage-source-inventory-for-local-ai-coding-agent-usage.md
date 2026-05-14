# Usage Source Inventory for Local AI Coding Agent Usage

AI Usage builds a Usage Source Inventory for each run from readable local Codex, Pi, and OpenCode artifacts instead of treating one Codex sessions directory as the only default Usage Source. A Custom Usage Source replaces default discovery for that run and is scanned as the entire inventory root; no default or sibling usage sources are added. This supersedes the single-Codex-source part of ADR-0002 while preserving Offline Usage, local artifacts as the authority, and no app-owned usage database.

## Considered Options

- Usage Source Inventory from all default local AI coding agent locations: chosen because the product scope includes Codex Usage, Pi Usage, and OpenCode Usage together.
- First readable Usage Source wins: rejected because it hides other readable AI Coding Agent Usage and conflicts with the product specification.
- Custom Usage Source plus default expansion: rejected because it makes a run-specific override surprising and weakens locality for tests and troubleshooting.
