# echo

A terminal RSVP (Rapid Serial Visual Presentation) reader, written in Rust.

RSVP flashes one word at a time in a fixed spot on the screen, so your eyes
stop jumping around and reading speed goes up. echo is aiming to do that for
plain text, web articles, ePubs, and eventually PDFs — with optional TTS
narration layered on top.

## Status

Early scaffolding. Nothing works yet. Follow along if you like watching
someone learn a new language in public.

## Planned features

- Read plain text and HTML articles from a URL or file
- RSVP playback with adjustable WPM, pause/resume, skip forward/back
- Pick the exact word to resume from
- Narration using macOS `say` / Linux TTS, ideally synced with the visual
- ePub and PDF support (later — PDFs are nasty)

## About this project

This is a fully AI-coded app. I'm a Java developer by day with some
TypeScript, Python, C, and C++ in my back pocket, but no Rust experience.
I'm using Claude Code to build this while learning the language — every
line of Rust here was written with heavy AI assistance, and part of the
point is to see how far that gets me.

So: don't take the code as idiomatic Rust yet. It's a learning log as much
as a tool.

## Inspiration

- **[readrrr](https://readrrr.com)** — my favorite RSVP app. The word-picking
  and narration features are what I'm chasing.
- **[speedread](https://github.com/pasky/speedread)** — Pasky's terminal
  RSVP reader in Perl. The spiritual ancestor of this project.

## License

MIT. See [LICENSE](./LICENSE).
