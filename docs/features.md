# Feature Roadmap

`nvmd` is intended to be a fast native Markdown companion for Neovim. The
features below focus on making the preview part of an editing workflow while
preserving its native Rust and `egui` approach.

## Current Foundation

Already available:

- Native preview window with automatic file reload.
- Markdown rendering for headings, paragraphs, lists, blockquotes, code
  blocks, horizontal rules, links, images as inline content, and tables.
- Viewer settings for document sizing, typography, and spacing.
- Native Mermaid rendering, including expanded diagram viewing, keyboard
  panning and zooming, and improved sequence and ER diagram layouts.

Table parsing and rendering are already implemented. Future table work should
improve usability, such as overflow behavior, copying, and large-table
navigation, rather than treating tables as a missing base feature.

## Highest-Value Features

### Neovim Integration and Cursor Synchronization

Initial support is available through the bundled `nvmd.nvim` Lua module:
commands launch and close the native viewer for the active Markdown buffer,
cursor movement scrolls the preview to its related rendered block, and plugin
managers can compile/install it through a standard Cargo build hook.

Implemented behavior:

- Provide `:NvmdOpen`, `:NvmdClose`, `:NvmdToggle`, and `:NvmdRefresh`.
- Follow the Neovim cursor by scrolling the preview to the related heading,
  paragraph, code block, table, or Mermaid block.
- Focus a Mermaid diagram when the cursor enters its fenced source block.
- Preview unsaved buffer edits through configurable live reload, enabled by
  default with a `150ms` debounce.
- Keep editing responsive: preview updates must not block Neovim or the
  terminal session.

Future improvements:

- Add bidirectional viewer-to-source jumps.
- Replace or extend file-based cursor updates with a richer command channel as
  interactive viewer actions expand.

Why it matters: this makes the viewer an editor companion instead of a
separate application that merely watches saved files.

### Preview-to-Source Navigation

Support navigation in the other direction: selecting rendered content in the
viewer should move Neovim to the corresponding source location.

Desired behavior:

- Jump to headings, paragraphs, code blocks, tables, and Mermaid source blocks.
- From an inline Mermaid rendering error, jump directly to the faulty fenced
  block.
- Use keyboard-first navigation alongside optional mouse selection.

Why it matters: two-way navigation removes the effort of locating source for
large documents and complicated diagrams.

### Native Syntax Highlighting

Render fenced code blocks with language-aware highlighting while remaining
native.

Initial languages:

- Rust
- TypeScript and JavaScript
- JSON
- SQL
- Bash
- TOML
- YAML

Desired behavior:

- Use the fence language tag when present and a readable plain-code fallback
  otherwise.
- Apply highlighting through viewer themes so code remains legible in dark
  and light modes.
- Avoid introducing a browser or Node.js runtime.

### Local Image Rendering

Display images referenced by Markdown rather than only presenting their
alternate text.

Desired behavior:

- Resolve relative paths against the Markdown document location.
- Render common local image formats with sensible maximum width constraints.
- Provide a readable missing-file or unsupported-format fallback.
- Support opening an image in a larger view with keyboard-controlled zoom.

### Cross-Platform Support and Packaging

Make Windows and Linux first-class supported targets rather than relying on
macOS testing.

Desired behavior:

- Add Windows and Linux font fallback paths, including Japanese-capable fonts.
- Replace macOS/Unix-specific diagnostic paths with platform config/cache
  locations.
- Exercise builds and tests on macOS, Windows, and Linux in CI.
- Publish installable binaries or documented package/install workflows.

Why it matters: `eframe` can support native windows across platforms, but font
selection and diagnostics require explicit portability work.

## Mermaid Features

### Minimap and Overview Mode

Large ER and sequence diagrams need orientation once they extend beyond one
viewport.

Desired behavior:

- Show a compact overview of the full diagram in expanded view.
- Indicate the visible region and update it as the user pans or zooms.
- Allow keyboard commands to toggle the minimap and reset to overview.

### Entity Focus and Relationship Tracing

Make dense ER diagrams inspectable without forcing users to visually trace all
connections at once.

Desired behavior:

- Select an entity using keyboard navigation.
- Highlight inbound and outbound relationships for the selected entity.
- Dim unrelated entities and edges while focus mode is active.
- Cycle connected entities or relationships without requiring the mouse.

### Diagram Export

Allow rendered Mermaid diagrams to leave the viewer as reusable assets.

Desired behavior:

- Export the selected Mermaid diagram to SVG.
- Export a rasterized PNG using the already rendered result.
- Preserve readable sizing and currently selected diagram content; viewport
  pan should not crop the export unless an explicit viewport-export option is
  later introduced.

## Quality-of-Life Features

### Document Outline

Provide an outline sidebar or popup generated from headings and Mermaid
blocks.

Desired behavior:

- Navigate with `j`/`k`, select with `Enter`, and filter by typing.
- Jump immediately to the selected section or diagram.
- Keep the outline keyboard-oriented and dismissible with `Esc`.

### Document Search

Support Vim-style search over rendered content.

Desired behavior:

- Use `/` to begin search.
- Highlight visible matches and navigate with `n` and `N`.
- Include headings, paragraph text, code, table content, and Mermaid source or
  searchable diagram labels where practical.

### Theme Presets

Expand viewer configuration beyond individual spacing sliders.

Desired behavior:

- Provide GitHub Dark, GitHub Light, and System presets.
- Configure Markdown colors, code highlighting, and Mermaid palette together
  so each preset is coherent.
- Retain manual typography and spacing controls where possible.

### Table Polish

Improve existing table functionality for technical documents.

Desired behavior:

- Handle wide tables without breaking the surrounding document layout.
- Add predictable horizontal overflow or scrolling behavior.
- Preserve readable alignment and support copying table content.

## Recommended Delivery Order

1. Extend the implemented `nvmd.nvim` cursor-follow path with preview-to-source jumps.
2. Add syntax highlighting and local image rendering.
3. Add Mermaid minimap, ER relationship focus, and diagram export.
4. Harden Windows/Linux support and produce distributable builds.
5. Add document search, outline navigation, theme presets, and table polish.

This order establishes the core editing workflow first, then improves document
fidelity and diagram analysis, and finally rounds out portability and everyday
navigation.
