import { cloudflare } from '@cloudflare/vite-plugin';
import { sites } from '@openai/sites-vite-plugin';
import { defineConfig } from 'vite';

export default defineConfig({
	publicDir: '.astro-dist',
	plugins: [
		sites(),
		cloudflare({
			viteEnvironment: { name: 'server' },
			config: {
				main: './worker/index.js',
				compatibility_date: '2026-08-21',
				compatibility_flags: ['nodejs_compat', 'assets_navigation_prefers_asset_serving'],
				assets: {
					binding: 'ASSETS',
					not_found_handling: '404-page',
					html_handling: 'auto-trailing-slash',
				},
			},
		}),
	],
});
