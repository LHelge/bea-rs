---
id: 5c28
title: Add shell completions
status: done
priority: P2
created: 2026-03-15T20:47:44.384453303Z
updated: 2026-03-15T23:38:52.855353105Z
tags:
- cli
- ux
depends_on:
- 35cd
---

Use clap_complete to generate completions for bash, zsh, and fish. Add a 'bea completions <shell>' subcommand. Document installation in README (e.g. eval "$(bea completions zsh)").