---
"@quality/cli": major
"@quality/action": major
"@quality/site": minor
---

Consolidate the v2 development line with reliable analyzer failures, bounded output,
local CI history and comparisons, and repository tool-version pins. Renamed
configuration and file-type changes now select the required checks. Concurrent
history writes preserve each run, and comparisons choose the preceding matching run.
Timed analyzers stop their process tree on expiry and preserve Unix terminal signal
behavior. A small Unix-only signal-handling dependency provides safe forwarding.

Refresh the documentation site to Lumen 5 and compatible stable build dependencies.
The development workspace requires Node.js 22.23.4 or newer. TypeScript 6 remains
required by the Astro checker and Action bundler; the Action runtime remains Node 24.
