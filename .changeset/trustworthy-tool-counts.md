---
"@quality/cli": patch
"@quality/action": patch
---

Add an additive `qualityExecuted` property to each SARIF run, marking whether
the tool's process actually started. A missing required tool still produces a
visible SARIF run and diagnostic, but is no longer counted in the Action's
`tools` output, which now reflects adapters that ran rather than every
reported run.

Fix a tool-cache collision in the Action: it resolved the release repository
and base URL after already checking the cache under a fixed `quality` key, so
a fork or fixture server publishing the same version string could silently
reuse another source's cached binary. The cache is now namespaced by the
resolved release base URL and looked up only after that URL is resolved.
