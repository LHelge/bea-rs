---
id: c2w
title: "Release notes: put breaking changes and their upgrade steps first (cliff.toml)"
status: done
priority: P3
created: "2026-11-20T20:17:55.731580870Z"
updated: "2026-11-20T22:01:58.245728264Z"
tags:
  - release
  - ci
attempts: 1
---

release.yml runs `git-cliff --latest` with no `cliff.toml`, so the notes come from commit subjects only: `BREAKING CHANGE:` footers are dropped and a breaking commit is listed inside its type's group (the 0.10 upgrade step lands under Documentation). Add a `cliff.toml` (`git-cliff --init` gives the default as a start) that lists breaking changes first and prints each footer. Changes the format of every future release, so decide the layout deliberately. Found while finishing 8ph.yg.