---
id: 95p
title: "Snapshot graph readability: edge routing, node width and size bounds"
status: done
priority: P3
created: "2026-11-20T20:56:30.443975261Z"
updated: "2026-11-20T22:24:23.500178629Z"
tags:
  - snapshot
attempts: 1
---

Cosmetic follow-ups found reviewing the snapshot renderers (bjt.4y, bjt.ne). HTML/SVG (`src/snapshot/html.rs`): two nodes in one cyclic layer that depend on each other are drawn as the same curve; an edge skipping layers is a straight Bézier that can pass behind the node boxes in between; the SVG grows without bound with layers and the largest layer (it scrolls); a node id line (`id · P · status`) can overflow the 220px box for deep dotted IDs. Markdown (`src/snapshot/markdown.rs`): an oversized graph part is not split further into nested epics; a part replaced by a "no room left" note does not set the Shortened banner; the document budget is spent first come first served, so on a very large store the ready table can crowd out the board. Also unverified in a real GitHub renderer: Mermaid numeric entity codes (`#35;`) in labels and the `[[…]]` epic shape — check once a real snapshot is posted.