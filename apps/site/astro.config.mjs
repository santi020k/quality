// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	outDir: './.astro-dist',
	integrations: [
		starlight({
			title: 'quality',
			description: 'One fast code-quality workflow for every project.',
			customCss: ['./src/styles/custom.css'],
			sidebar: [
				{
					label: 'Start',
					items: [
						{ label: 'Overview', slug: 'overview' },
						{ label: 'Getting started', slug: 'getting-started' },
					],
				},
				{
					label: 'Use quality',
					items: [
						{ label: 'Commands', slug: 'commands' },
						{ label: 'Configuration', slug: 'configuration' },
						{ label: 'Changed files and baselines', slug: 'changed-files-and-baselines' },
					],
				},
				{
					label: 'Integrate',
					items: [
						{ label: 'GitHub Actions', slug: 'github-actions' },
						{ label: 'Built-in and custom adapters', slug: 'adapters' },
					],
				},
			],
		}),
	],
});
