interface DocsItem {
	label: string;
	slug: string;
}

interface DocsGroup {
	label: string;
	items: readonly DocsItem[];
}

export const docsGroups: readonly DocsGroup[] = [
	{
		label: 'Start',
		items: [
			{ label: 'Overview', slug: 'overview' },
			{ label: 'Getting started', slug: 'getting-started' },
			{ label: 'Language-aware presets', slug: 'presets' },
		],
	},
	{
		label: 'Use quality',
		items: [
			{ label: 'Commands', slug: 'commands' },
			{ label: 'Configuration', slug: 'configuration' },
			{ label: 'Changed files & baselines', slug: 'changed-files-and-baselines' },
		],
	},
	{
		label: 'Integrate',
		items: [
			{ label: 'GitHub Actions', slug: 'github-actions' },
			{ label: 'Local CI & PR gates', slug: 'local-ci' },
			{ label: 'AI coding agents', slug: 'ai-agents' },
			{ label: 'Built-in & custom adapters', slug: 'adapters' },
			{ label: 'Compatibility & support', slug: 'compatibility' },
		],
	},
];

export const docsItems = docsGroups.flatMap((group) => group.items);
