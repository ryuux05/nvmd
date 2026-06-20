# Product

## Register

product

## Users

Neovim power users who write Markdown daily (docs, notes, READMEs, changelogs). They live in a terminal, use a tiling WM or tmux, and have a carefully tuned editor environment. The preview window sits beside their editor and should feel like it belongs there, not like a browser app dropped into the workspace.

## Product Purpose

nvmd opens a lightweight native Markdown preview window that watches the current file and reloads on save. No Electron, no Chromium, no browser dependency. It renders Markdown and Mermaid diagrams natively via egui and stays out of the way. Success looks like: the user forgets it is a separate application.

## Brand Personality

Precise, quiet, editor-native. The window should feel like a high-quality Neovim theme rendered as a viewer, not a standalone product.

## Anti-references

- VS Code / GitHub dark (generic blue-grey, every dev tool looks like this)
- SaaS dashboards (rounded cards, colorful gradients, product-y)
- Plain egui defaults (flat grey, no visual hierarchy, widget-library look)

## Design Principles

1. **Belongs in the workspace** — styling follows high-quality Neovim themes (Tokyo Night, Catppuccin Macchiato). Deep navy-blacks, blue-tinted neutrals, intentional accent.
2. **Chrome recedes, content leads** — the header, settings, and palette are visually subordinate to the document. They appear when needed and disappear into the background otherwise.
3. **Every pixel earns its place** — no decorative elements. Visual structure should map to semantic structure: headings have weight, code has depth, blockquotes have voice.
4. **Keyboard-first, mouse-tolerated** — interactions are optimized for keyboard. Visual affordances reinforce keyboard metaphors (keycap labels, command palette).
5. **Quiet by default, clear when active** — selected/active states are unmistakable; idle states are nearly invisible.

## Accessibility & Inclusion

- Body text contrast ≥ 4.5:1 against page background
- Muted text used only for decorative or secondary labels, never primary content
- No reduced-motion risk (egui uses immediate mode; no CSS animations)
