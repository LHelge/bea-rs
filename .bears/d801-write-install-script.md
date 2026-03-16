---
id: d801
title: Write install script
status: done
priority: P2
created: 2026-03-15T20:47:09.884481410Z
updated: 2026-03-16T19:00:49.489968Z
tags:
- release
- docs
depends_on:
- ea65
---

Shell script at install.sh that detects OS/arch, downloads the correct release binary from GitHub releases, and installs to /usr/local/bin. Fallback to cargo install if no binary available.