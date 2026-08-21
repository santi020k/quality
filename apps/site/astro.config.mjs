// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	site: 'https://quality-cli.santi020k.chatgpt.site',
	outDir: './.astro-dist',
	integrations: [
		starlight({
			title: 'quality',
			description: 'One fast code-quality workflow for every project.',
			customCss: ['./src/styles/custom.css'],
			head: [
				{ tag: 'meta', attrs: { property: 'og:type', content: 'website' } },
				{ tag: 'meta', attrs: { property: 'og:title', content: 'quality' } },
				{ tag: 'meta', attrs: { property: 'og:description', content: 'One quality command for every project.' } },
				{ tag: 'meta', attrs: { property: 'og:image', content: 'https://quality-cli.santi020k.chatgpt.site/og.png' } },
				{ tag: 'meta', attrs: { name: 'twitter:card', content: 'summary_large_image' } },
				{ tag: 'meta', attrs: { name: 'twitter:title', content: 'quality' } },
				{ tag: 'meta', attrs: { name: 'twitter:description', content: 'One quality command for every project.' } },
				{ tag: 'meta', attrs: { name: 'twitter:image', content: 'https://quality-cli.santi020k.chatgpt.site/og.png' } },
			],
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
