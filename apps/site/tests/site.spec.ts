import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';
import { docsItems } from '../src/lib/docs-navigation';

declare global {
	interface Window { qualityNavigationProbe?: string; }
}

for (const theme of ['light', 'dark']) {
	for (const route of ['/', ...docsItems.map((item) => `/${item.slug}/`)]) {
		test(`${theme}: ${route} is readable, accessible, and contained`, async ({ page }) => {
			const errors: string[] = [];
			page.on('pageerror', (error) => errors.push(error.message));
			await page.addInitScript((value) => localStorage.setItem('quality-theme', value), theme);
			await page.goto(route);
			await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
			await expect(page.locator('main h1')).toHaveCount(1);
			await expect(page.locator('main h1')).toBeVisible();
			await expect(page.locator('.ui-icon:empty')).toHaveCount(0);
			expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
			if (route !== '/') {
				await expect(page.locator('.docs-prose')).toBeVisible();
				for (const item of docsItems) await expect(page.locator('.docs-sidebar').getByRole('link', { name: item.label, exact: true, includeHidden: true })).toHaveAttribute('href', `/${item.slug}/`);
				const markdown = await page.request.get(`${route.slice(0, -1)}.md`);
				expect(markdown.ok()).toBe(true);
			}
			const audit = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']).analyze();
			expect(audit.violations).toEqual([]);
			expect(errors).toEqual([]);
		});
	}
}

test('theme and interactive controls survive client navigation and history', async ({ page }, testInfo) => {
	await page.goto('/');
	await page.evaluate(() => { window.qualityNavigationProbe = 'alive'; });
	await page.getByRole('button', { name: 'Toggle color theme' }).click();
	const theme = await page.locator('html').getAttribute('data-theme');
	await page.getByRole('link', { name: 'Get started', exact: true }).click();
	await expect(page).toHaveURL(/\/getting-started\//);
	expect(await page.evaluate(() => window.qualityNavigationProbe)).toBe('alive');
	await expect(page.locator('html')).toHaveAttribute('data-theme', theme ?? 'dark');
	if (testInfo.project.name === 'mobile') {
		await page.getByRole('button', { name: 'Open navigation' }).click();
		const menu = page.getByRole('dialog', { name: 'Find your next step.' });
		await expect(menu).toBeVisible();
		await menu.getByRole('link', { name: 'Language-aware presets', exact: true }).click();
	} else {
		await page.locator('.docs-sidebar').getByRole('link', { name: 'Language-aware presets', exact: true }).click();
	}
	await expect(page).toHaveURL(/\/presets\//);
	await expect(page.locator('body')).not.toHaveCSS('overflow', 'hidden');
	await page.goBack();
	await expect(page).toHaveURL(/\/getting-started\//);
	await page.getByRole('button', { name: 'Toggle color theme' }).click();
	await expect(page.locator('html')).not.toHaveAttribute('data-theme', theme ?? 'dark');
});

test('mobile menu accepts pointer and keyboard input before the dialog controller loads', async ({ page }, testInfo) => {
	test.skip(testInfo.project.name !== 'mobile', 'Mobile navigation contract');
	const errors: string[] = [];
	page.on('pageerror', (error) => errors.push(error.message));
	let releaseController = () => {};
	const controllerGate = new Promise<void>((resolve) => { releaseController = resolve; });
	await page.route('**/_astro/dialogs.*.js', async (route) => {
		await controllerGate;
		await route.continue();
	});
	const controllerRequested = page.waitForRequest('**/_astro/dialogs.*.js');
	try {
		await page.goto('/', { waitUntil: 'domcontentloaded' });
		await controllerRequested;
		const trigger = page.getByRole('button', { name: 'Open navigation' });
		const menu = page.getByRole('dialog', { name: 'Find your next step.' });
		await trigger.click();
		await expect(menu).toBeVisible();
		expect(await menu.evaluate((element) => element.matches(':modal'))).toBe(true);
		await menu.getByRole('button', { name: 'Close navigation' }).click();
		await expect(menu).not.toBeVisible();
		await trigger.focus();
		await page.keyboard.press('Enter');
		await expect(menu).toBeVisible();
		await page.keyboard.press('Escape');
		await expect(menu).not.toBeVisible();
		await expect(trigger).toBeFocused();
		const controllerLoaded = page.waitForResponse('**/_astro/dialogs.*.js');
		releaseController();
		await controllerLoaded;
		await page.waitForLoadState('networkidle');
		// Simulate a browser without native commands after Lumen has loaded.
		for (const button of [trigger, page.getByRole('button', { name: 'Close navigation', includeHidden: true })]) {
			await button.evaluate((element) => {
				element.removeAttribute('command');
				element.removeAttribute('commandfor');
			});
		}
		await trigger.click();
		await expect(menu).toBeVisible();
		await menu.getByRole('button', { name: 'Close navigation' }).click();
		await expect(menu).not.toBeVisible();
		expect(errors).toEqual([]);
	} finally {
		releaseController();
	}
});

test('mobile menu isolates background focus, escapes, and opens again after a page swap', async ({ page }, testInfo) => {
	test.skip(testInfo.project.name !== 'mobile', 'Mobile navigation contract');
	await page.goto('/');
	const trigger = page.getByRole('button', { name: 'Open navigation' });
	await trigger.click();
	const menu = page.getByRole('dialog', { name: 'Find your next step.' });
	await expect(menu).toBeVisible();
	expect(await menu.evaluate((element) => element.matches(':modal'))).toBe(true);
	const close = menu.getByRole('button', { name: 'Close navigation' });
	await expect(close).toBeInViewport({ ratio: 1 });
	await expect(menu.getByRole('heading', { name: 'Find your next step.' })).toBeInViewport({ ratio: 1 });
	const audit = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']).analyze();
	expect(audit.violations).toEqual([]);
	await expect(close).toBeFocused();
	await trigger.evaluate((element) => element.focus());
	await expect(close).toBeFocused();
	for (const direction of ['Tab', 'Shift+Tab']) {
		await (direction === 'Tab' ? menu.getByRole('link').last() : close).focus();
		for (let index = 0; index < 3; index++) {
			await page.keyboard.press(direction);
			// Native modal dialogs may visit browser chrome, but never the background page.
			expect(await menu.evaluate((element) => element.contains(document.activeElement)
				|| (document.activeElement === document.body && !document.hasFocus()))).toBe(true);
		}
		expect(await menu.evaluate((element) => element.contains(document.activeElement))).toBe(true);
	}
	await close.focus();
	await page.keyboard.press('Escape');
	await expect(menu).not.toBeVisible();
	await expect(trigger).toBeFocused();
	await trigger.click();
	await menu.getByRole('link', { name: 'Commands', exact: true }).click();
	await expect(page).toHaveURL(/\/commands\//);
	await page.getByRole('button', { name: 'Open navigation' }).click();
	await expect(page.getByRole('dialog')).toBeVisible();
	await page.getByRole('button', { name: 'Close navigation' }).click();
	await expect(page.getByRole('dialog')).not.toBeVisible();
});

test('docs anchors, source links and protected external links retain their targets', async ({ page }) => {
	await page.goto('/getting-started/');
	const toc = page.locator('.docs-toc a[href^="#"]').first();
	if (await toc.isVisible()) {
		const href = await toc.getAttribute('href');
		await toc.click();
		expect(new URL(page.url()).hash).toBe(href);
	}
	const links = page.locator('.docs-prose a[href^="https://"]');
	for (const link of await links.all()) {
		const href = await link.getAttribute('href');
		if (href && new URL(href).origin !== 'https://quality.santi020k.com') {
			await expect(link).toHaveAttribute('target', '_blank');
			await expect(link).toHaveAttribute('rel', /noopener/);
			await expect(link).toHaveAttribute('rel', /noreferrer/);
		}
	}
});

test('320px and enlarged text keep the entire page within the viewport', async ({ page }) => {
	await page.setViewportSize({ width: 320, height: 700 });
	for (const route of ['/', '/adapters/', '/commands/']) {
		await page.goto(route);
		await page.evaluate(() => { document.documentElement.style.fontSize = '125%'; });
		await expect(page.locator('html')).toHaveCSS('font-size', '20px');
		if (route === '/') {
			const heading = await page.locator('.hero-title').evaluate((element) => ({ width: element.clientWidth, content: element.scrollWidth }));
			expect(heading.content, 'The enlarged heading must fit its own container').toBeLessThanOrEqual(heading.width);
		}
		const layout = await page.evaluate(() => ({
			width: document.documentElement.scrollWidth,
			overflow: [...document.querySelectorAll('body *')]
				.filter((element) => element.getBoundingClientRect().right > innerWidth)
				.map((element) => ({
					tag: element.tagName,
					class: element.getAttribute('class'),
					right: element.getBoundingClientRect().right,
				})),
		}));
		expect(layout.width, `${route} overflows at 320px with enlarged text: ${JSON.stringify(layout.overflow)}`).toBeLessThanOrEqual(320);
	}
});

test('Lumen command tabs support keyboard selection and copying after navigation', async ({ page, context }) => {
	await context.grantPermissions(['clipboard-read', 'clipboard-write']);
	await page.goto('/');
	const first = page.getByRole('tab', { name: 'First run', exact: true });
	await first.focus();
	await page.keyboard.press('ArrowRight');
	const changes = page.getByRole('tab', { name: 'Your changes', exact: true });
	await expect(changes).toHaveAttribute('aria-selected', 'true');
	await expect(page.getByRole('tabpanel').filter({ visible: true })).toContainText('quality format --changed');
	await page.getByRole('tabpanel').filter({ visible: true }).getByRole('button').click();
	await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toContain('quality format --changed');
	await page.getByRole('link', { name: 'Get started', exact: true }).click();
	await expect(page).toHaveURL(/\/getting-started\//);
	await expect(page.locator('main h1')).toHaveText('Getting started');
	await page.goBack();
	await expect(page).toHaveURL('/');
	await page.getByRole('tab', { name: 'A useful report', exact: true }).click();
	await expect(page.getByRole('tabpanel').filter({ visible: true })).toContainText('quality check --format agent');
});

test('motion-enabled navigation retains readable content and working controls', async ({ page }) => {
	await page.emulateMedia({ reducedMotion: 'no-preference' });
	await page.goto('/');
	await expect(page.locator('.hero-preview')).toBeVisible();
	await page.getByRole('link', { name: 'Get started', exact: true }).click();
	await expect(page.locator('.docs-prose')).toBeVisible();
	await page.getByRole('button', { name: 'Toggle color theme' }).click();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});

test('Markdown syntax highlighting follows the selected theme', async ({ page }) => {
	await page.addInitScript(() => localStorage.setItem('quality-theme', 'light'));
	await page.goto('/getting-started/');
	const code = page.locator('.docs-prose pre').first();
	const light = await code.evaluate((element) => getComputedStyle(element).backgroundColor);
	await page.getByRole('button', { name: 'Toggle color theme' }).click();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
	await expect.poll(() => code.evaluate((element) => getComputedStyle(element).backgroundColor)).not.toBe(light);
});

test('theme survives navigation when browser storage is unavailable', async ({ page }) => {
	await page.emulateMedia({ colorScheme: 'light' });
	await page.addInitScript(() => {
		Storage.prototype.getItem = () => { throw new DOMException('Storage is blocked', 'SecurityError'); };
		Storage.prototype.setItem = () => { throw new DOMException('Storage is blocked', 'SecurityError'); };
	});
	await page.goto('/');
	await page.getByRole('button', { name: 'Toggle color theme' }).click();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
	await page.getByRole('link', { name: 'Get started', exact: true }).click();
	await expect(page).toHaveURL(/\/getting-started\//);
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
	await page.getByRole('button', { name: 'Toggle color theme' }).click();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
});
