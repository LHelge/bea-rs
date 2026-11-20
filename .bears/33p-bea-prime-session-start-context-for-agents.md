---
id: "33p"
title: "bea prime: session-start context for agents"
status: open
priority: P3
created: "2026-11-20T12:52:25.770721243Z"
updated: "2026-11-20T12:52:25.770721243Z"
tags:
  - agents
  - cli
parent: "8z2"
---

Prints compact workflow context: ready queue (top N), in-progress tasks with assignee and attempts, review queue, and a short reminder of the workflow verbs. Meant for a coding agent's session-start hook. Consider scaffolding the hook from `bea init --claude`.