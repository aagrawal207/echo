# CLAUDE.md — echo

Project-specific instructions for Claude Code sessions working on this repo.

## Context

- Pure Rust binary crate. Author is learning Rust; assume no prior Rust
  knowledge and favor explanation when introducing new concepts
  (ownership, lifetimes, traits, `Result`/`Option` patterns, iterators,
  cargo workspace mechanics).
- Author's background: Java (primary), some TypeScript / Python / C / C++.
  Analogies to Java are welcome when they clarify (e.g. "like Java's
  `Optional` but checked at compile time").
- Fully AI-coded project. That is stated publicly in the README — don't
  strip the disclosure.
- Cross-platform target: Linux (EC2 dev box) and macOS (primary daily
  driver). Avoid platform-specific code in core logic; gate OS-specific
  bits (e.g. TTS) behind `#[cfg(target_os = "...")]` or trait abstractions.

## Inspirations

- [readrrr](https://readrrr.com) — feature target
- [speedread](https://github.com/pasky/speedread) — reference implementation
  in Perl; read its source for algorithmic ideas (word timing, ORP
  alignment, punctuation-aware pauses).

## Build & run

```bash
cargo build
cargo run -- <args>
cargo test
cargo fmt
cargo clippy -- -D warnings
```

Run `cargo fmt` and `cargo clippy` before every commit.

## Coding conventions

- Stable Rust, edition 2024.
- Prefer `anyhow` for application errors, `thiserror` for library-style
  errors — but don't pull those in until we actually need them.
- Keep `main.rs` thin. Move logic into modules under `src/` as features
  grow (`src/rsvp.rs`, `src/source/`, `src/tts/`, etc.).
- No `unwrap()` / `expect()` in library code. In `main` and tests it's
  fine.
- Small commits, one feature or refactor per commit. Follow the
  Conventional Commits format from the global CLAUDE.md.

## Feature roadmap (build in this order)

1. Read plain text from stdin or file, tokenize into words, play RSVP
   at a configurable WPM in the terminal.
2. Pause / resume / skip controls via keyboard.
3. Fetch and extract article text from a URL (HTML → readable text).
4. "Start from this word" picker.
5. TTS narration (macOS `say`, Linux `espeak` or similar).
6. ePub support.
7. PDF support (last — painful ecosystem).

Don't jump ahead. Each step should ship as a working, committed increment.

## What to ask before changing

- Adding a new crate dependency — flag it first, explain why, suggest
  alternatives.
- Introducing async runtimes (tokio etc.) — big commitment, discuss before.
- Anything that changes cross-platform behavior.

## Don't

- Don't `git push`. Author pushes manually.
- Don't add hypothetical abstractions "for later." Build for the current
  step only.
- Don't add comments explaining Rust syntax in committed code — those
  belong in conversation, not the source.
