import type { CollectionEntry } from 'astro:content';

type DocEntry = CollectionEntry<'docs'>;

const preferredOrder = [
	'overview',
	'getting-started',
	'commands',
	'configuration',
	'changed-files-and-baselines',
	'github-actions',
	'ai-agents',
	'adapters',
];

const orderById = new Map(preferredOrder.map((id, index) => [id, index]));

export const sortDocs = (entries: DocEntry[]) =>
	entries.toSorted((left, right) => {
		const leftIndex = orderById.get(left.id) ?? Number.MAX_SAFE_INTEGER;
		const rightIndex = orderById.get(right.id) ?? Number.MAX_SAFE_INTEGER;

		return leftIndex - rightIndex || left.id.localeCompare(right.id);
	});

export const renderLlmsIndex = (site: URL, entries: DocEntry[]) => {
	const documentation = sortDocs(entries)
		.map((entry) => `- [${entry.data.title}](${new URL(`/${entry.id}.md`, site)}): ${entry.data.description}`)
		.join('\n');

	return `# quality

> One fast, predictable code-quality workflow for Rust, Swift, Android/Kotlin, JavaScript, TypeScript, Astro, content, and GitHub Actions repositories.

quality is a deterministic local CLI. It detects and runs established ecosystem analyzers; its checking path does not call an AI service.

## Documentation

${documentation}

## Machine-readable resources

- [Complete documentation](${new URL('/llms-full.txt', site)}): All documentation in one Markdown document.
- [quality.yml JSON Schema](${new URL('/quality.schema.json', site)}): Configuration schema for validation and editor assistance.

## Source

- [GitHub repository](https://github.com/santi020k/quality): Source, releases, contributing guidance, and security policy.
`;
};

export const renderLlmsFull = (entries: DocEntry[]) => {
	const documentation = sortDocs(entries)
		.map((entry) => `# ${entry.data.title}\n\n> ${entry.data.description}\n\n${entry.body?.trim() ?? ''}`)
		.join('\n\n---\n\n');

	return `# quality documentation

> Complete documentation for the quality CLI, optimized for coding agents and other language-model tools.

quality is deterministic: it runs repository-local analyzers and never calls an AI service as part of the checking path.

${documentation}
`;
};
