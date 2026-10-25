// Preserve the selected theme on the incoming document before Astro swaps it in.
document.addEventListener('astro:before-swap', (event) => {
	if ('newDocument' in event && event.newDocument instanceof Document) {
		const theme = document.documentElement.dataset.theme;
		if (theme === 'light' || theme === 'dark') event.newDocument.documentElement.dataset.theme = theme;
	}
});

// Lumen owns dialog focus, dismissal, scroll locking, and theme interactions.
// Close an open menu before navigation so the next page starts outside a modal.
document.addEventListener('astro:before-preparation', () => {
	const menu = document.querySelector<HTMLDialogElement>('#site-menu');
	if (menu?.open) menu.close();
});

let readingObserver: ResizeObserver | undefined;
const enhanceReading = () => {
	readingObserver?.disconnect();
	const update = (element: HTMLElement) => {
		if (element.scrollWidth > element.clientWidth) {
			element.tabIndex = 0;
			if (element.tagName === 'PRE') {
				element.setAttribute('role', 'region');
				element.setAttribute('aria-label', 'Scrollable code example');
			}
		} else {
			element.removeAttribute('tabindex');
			if (element.tagName === 'PRE') {
				element.removeAttribute('role');
				element.removeAttribute('aria-label');
			}
		}
	};
	readingObserver = new ResizeObserver((entries) => {
		for (const entry of entries) if (entry.target instanceof HTMLElement) update(entry.target);
	});
	for (const element of document.querySelectorAll<HTMLElement>('.docs-prose pre, .docs-prose table')) {
		update(element);
		readingObserver.observe(element);
	}
};
document.addEventListener('astro:page-load', enhanceReading);
document.addEventListener('astro:before-swap', () => readingObserver?.disconnect());
