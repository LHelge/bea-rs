---
id: bjt.4y
title: Self-contained HTML renderer with SVG graph
status: done
priority: P2
created: "2026-11-20T12:52:25.752822361Z"
updated: "2026-11-20T20:48:09.417402719Z"
tags:
  - snapshot
depends_on:
  - bjt.zz
attempts: 1
---

Single HTML file, template embedded via include_str!, inline CSS, no external requests. Sections: header stats, epic progress bars, status board, dependency graph as SVG laid out in Rust from the layer data, recent archive log. Light/dark via prefers-color-scheme. HTML-escape all task content.