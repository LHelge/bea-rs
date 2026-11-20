---
id: zhk
title: Check the snapshot's Mermaid output in a real GitHub renderer
status: open
priority: P3
created: "2026-11-20T22:27:34.191015630Z"
updated: "2026-11-20T22:27:34.191015630Z"
tags:
  - snapshot
  - manual
---

Split from 95p, whose renderer items are done. Still unverified in a real GitHub renderer: the Mermaid numeric entity codes (`#35;`) in node labels, and the `[[…]]` epic node shape. Post a real snapshot (the `snapshot.yml` job summary of a pull request does) with titles containing quotes, brackets, `;`, `#` and backticks, and an epic that is a graph node, and look at the rendered diagram. Fix `src/snapshot/markdown.rs` if either does not render. Tagged `manual`: it needs a person looking at GitHub.