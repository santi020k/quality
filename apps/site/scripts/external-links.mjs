// @ts-check

/** Keep native Markdown navigation while protecting external destinations. */
export default function externalLinks() {
	/** @satisfies {NonNullable<import('@astrojs/markdown-satteri').SatteriProcessorOptions['hastPlugins']>[number]} */
	const plugin = {
		name: 'quality-external-links',
		element: {
			filter: ['a'],
			visit(node, context) {
				const { href, rel, download } = node.properties;
				if (typeof href !== 'string' || !/^https?:\/\//i.test(href) || download !== undefined) return;
				let destination;
				try { destination = new URL(href); } catch { return; }
				if (destination.origin === 'https://quality.santi020k.com') return;
				const existing = rel ?? [];
				context.setProperty(node, 'target', '_blank');
				context.setProperty(node, 'rel', [...new Set([...existing, 'noopener', 'noreferrer'])]);
			},
		},
	};
	return plugin;
}
