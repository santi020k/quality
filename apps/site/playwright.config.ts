import { defineConfig } from '@playwright/test';

const port = 4494;
export default defineConfig({
	testDir: './tests',
	fullyParallel: true,
	timeout: 60_000,
	forbidOnly: Boolean(process.env.CI),
	retries: 0,
	workers: 2,
	reporter: 'list',
	outputDir: './test-results',
	use: {
		baseURL: `http://127.0.0.1:${port}`,
		trace: 'retain-on-failure',
		reducedMotion: 'reduce',
		launchOptions: process.env.PW_CHROMIUM_CHANNEL ? { channel: process.env.PW_CHROMIUM_CHANNEL } : {},
	},
	projects: [
		{ name: 'desktop', use: { viewport: { width: 1440, height: 1000 } } },
		{ name: 'mobile', use: { viewport: { width: 375, height: 812 }, isMobile: true, hasTouch: true } },
	],
	webServer: {
		command: `pnpm exec astro preview --host 127.0.0.1 --port ${port}`,
		url: `http://127.0.0.1:${port}`,
		reuseExistingServer: !process.env.CI,
		timeout: 60_000,
	},
});
