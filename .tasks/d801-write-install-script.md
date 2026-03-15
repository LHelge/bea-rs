---
id: d801
title: Write install script
status: open
priority: P2
created: 2026-03-15T20:47:09.884481410Z
updated: 2026-03-15T20:48:39.947403379Z
tags:
- release
- docs
depends_on:
- ea65
---

Shell script at install.sh that detects OS/arch, downloads the correct release binary from GitHub releases, and installs to /usr/local/bin. Fallback to cargo install if no binary available.