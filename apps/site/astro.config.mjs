// @ts-check
import { defineConfig } from 'astro/config';
import { satteri } from '@astrojs/markdown-satteri';
import externalLinks from './scripts/external-links.mjs';

export default defineConfig({
	site: 'https://quality.santi020k.com',
	outDir: './.astro-dist',
	// Keep Lumen's optional phone assets lazy and independently cacheable.
	vite: {
		build: {
			rolldownOptions: { output: { codeSplitting: { groups: [
				{ name: 'lumen-phone-flags', test: /phone-flags\.generated\.js/ },
			] } } },
		},
	},
	markdown: {
		processor: satteri({ hastPlugins: [externalLinks()] }),
		shikiConfig: { themes: { light: 'github-light', dark: 'github-dark-default' } },
	},
});
