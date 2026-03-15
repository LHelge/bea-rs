---
id: 5acc
title: Add import/export commands
status: open
priority: P3
created: 2026-03-15T20:47:58.538872341Z
updated: 2026-03-15T20:48:40.200906001Z
tags:
- feature
depends_on:
- ea65
---

bea export --format json > tasks.json and bea export --format csv > tasks.csv. Also bea import tasks.json to bulk-create tasks from a JSON array (same schema as --json output). Useful for migrating between projects or seeding from a spec.