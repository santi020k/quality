---
"@quality/cli": patch
---

Fix two correctness bugs:

- Atomic writes now preserve an existing destination's exact permissions
  (e.g. a deliberately narrowed 0600, or a shared 0640/0664) instead of
  always requesting the temp-file crate's owner-only (0600) default or a
  fixed umask-respecting mode. New files still get the umask-respecting mode
  a plain write would produce.
- `--changed` detection with a nested `--root` (a subdirectory of a larger
  Git repository) no longer mixes up Git's two path conventions: `git diff
  --name-only` reports paths relative to the repository's top-level
  directory while `git ls-files` reports paths relative to `--root`. This
  previously produced broken paths and let sibling-directory changes leak
  into the report; changed-file detection is now scoped to `--root` and its
  paths are reported relative to it.
