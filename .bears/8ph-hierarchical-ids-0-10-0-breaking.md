---
id: 8ph
title: Hierarchical IDs (0.10.0, breaking)
type: epic
status: done
priority: P1
created: "2026-11-20T12:52:25.707376031Z"
updated: "2026-11-20T20:25:06.618960540Z"
tags:
  - ids
  - breaking
---

IDs encode the parent chain: `a1b` root, `a1b.c2` child, `a1b.c2.d7` grandchild. Every segment is random alphanumeric.

Decisions:
- Root segment uses `id-length` (default 3). Child segments are 2 chars at every depth.
- A child segment grows by one char when the parent's sibling space is more than ~25% full. Existing IDs keep their length.
- Child length is configurable in .bears.yml next to `id-length`.
- The `parent` frontmatter field is dropped; parent is derived from the ID.
- Reparenting renames the task and its subtree and rewrites all references.
- Filenames stay `{id}-{slug}.md`; '-' sorts before '.', so a parent lists first with its children after it.