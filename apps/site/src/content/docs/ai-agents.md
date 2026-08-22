---
title: AI coding agents
description: Give coding agents a deterministic quality workflow without adding an AI service.
---

`quality` remains deterministic when an AI coding agent invokes it. The agent
runs the same local analyzers, configuration, and repository tasks as a human or
CI job; checking does not call an AI service.

## Read the documentation programmatically

The documentation site publishes AI-readable resources that stay synchronized
with these pages during every build:

- [`/llms.txt`](/llms.txt) is the compact documentation index.
- [`/llms-full.txt`](/llms-full.txt) contains the complete documentation in one
  Markdown document.
- Every documentation page is also available as Markdown by replacing its
  trailing slash with `.md`, such as [`/commands.md`](/commands.md).

`/llm.txt` is provided as a compatibility copy for tools that look for the
singular filename. New integrations should prefer the standard `/llms.txt`.

## Add repository instructions

Print a ready-to-paste `AGENTS.md` section:

```bash
quality instructions --format agents
```

Add the output to the consumer repository's `AGENTS.md`. The command only
prints Markdown: it never creates or modifies the instruction file.

Keeping the instructions in the consuming repository makes its quality policy
visible to developers and compatible agents. Run the command again after a
`quality` upgrade to review the current recommendation.

## Use structured output

Agents and automation can inspect configuration and results without parsing
the human-readable terminal presentation:

```bash
quality doctor --format json
quality check --format json
quality check --changed --format json
```

Use `quality check --changed` for iteration and the complete `quality check`
before handoff. Use `quality fix` only when edits are intended, and inspect the
resulting changes afterward.

## Configure `quality.yml` safely

Generated configuration declares the published
[`quality.yml` JSON Schema](/quality.schema.json). Editors that support the
YAML language-server schema comment can validate keys and values while a human
or agent edits the file.

Preview detected configuration without writing a file:

```bash
quality init --dry-run
```

Unknown keys are rejected by the CLI as well, so the schema improves authoring
but never replaces runtime validation.

## Why there is no MCP server

Coding agents with terminal access can already invoke the local `quality` CLI.
An MCP wrapper would duplicate that interface without changing the checking
path. MCP becomes useful only if `quality` later needs to expose remote data or
operations, such as organization policies, historical reports, or
cross-repository queries.
