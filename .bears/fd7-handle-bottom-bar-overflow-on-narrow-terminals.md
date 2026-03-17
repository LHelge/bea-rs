---
id: fd7
title: Handle bottom bar overflow on narrow terminals
status: done
priority: P3
created: 2026-03-17T10:17:47.763440Z
updated: 2026-03-17T12:12:25.611304Z
tags:
- tui
- ux
- polish
parent: vbj
---

Address visual overflow risk in shortcut hint bar on narrow terminal widths.

## Problem
Bottom-bar shortcut rendering can overflow or truncate awkwardly when terminal width is limited.

## Scope
- Define expected behavior for narrow widths (wrap, elide, paginate, or compact hint set).
- Implement the chosen behavior without reducing discoverability on normal widths.
- Verify on small terminal sizes.

## Acceptance Criteria
- Bottom bar remains readable and stable on narrow terminals.
- No rendering artifacts or panic conditions from compact layouts.
