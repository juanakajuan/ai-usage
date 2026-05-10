# Ratatui and Crossterm for Terminal Interface

AI Usage is a Rust terminal application whose first version needs a modern, keyboard-driven interface with compact summaries, session detail, reload behavior, and theme-aware styling. We will build the terminal interface with Ratatui for layout and widgets, backed by Crossterm for terminal input and rendering, because that combination is the standard Rust TUI path and keeps the app portable without inventing a terminal abstraction.

## Considered Options

- Ratatui with Crossterm: chosen for mature Rust TUI primitives, active ecosystem usage, and cross-terminal portability.
- Hand-written terminal control: rejected because it would push layout, input, and rendering complexity into application code.
- Immediate-mode graphical UI: rejected because the product direction is explicitly terminal-first.
