import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';

import { createCards, pathnameOutput } from '@santi020k/og';
import { definePageMetadata } from '@santi020k/og/metadata';
import { definePresetConfig } from '@santi020k/og/presets';

const root = path.resolve(import.meta.dirname, '..');
const docsDirectory = path.join(root, 'src', 'content', 'docs');

const readFrontmatterField = (source, field) => {
	const match = source.match(new RegExp(`^${field}:\\s*(.+?)\\s*$`, 'm'));
	const value = match?.[1]?.trim();

	if (!value) throw new Error(`Missing ${field} in documentation frontmatter.`);

	const quote = value.at(0);

	return quote && quote === value.at(-1) && (quote === '"' || quote === "'") ? value.slice(1, -1) : value;
};

const docs = await Promise.all(
	(await readdir(docsDirectory))
		.filter((fileName) => fileName.endsWith('.md') || fileName.endsWith('.mdx'))
		.sort()
		.map(async (fileName) => {
			const source = await readFile(path.join(docsDirectory, fileName), 'utf8');
			const slug = fileName.replace(/\.mdx?$/, '');
			const title = readFrontmatterField(source, 'title');

			return definePageMetadata({
				badge: 'Documentation',
				description: readFrontmatterField(source, 'description'),
				image: { alt: `${title} — quality code-quality CLI`, output: pathnameOutput(`/${slug}/`) },
				pathname: `/${slug}/`,
				schemaTypes: ['TechArticle'],
				title,
			});
		}),
);

const pages = [
	definePageMetadata({
		badge: 'Code quality, orchestrated',
		description: 'Run trusted code-quality tools across programming ecosystems through one deterministic workflow.',
		image: { alt: 'One command. Every tool. — quality code-quality CLI', output: 'index.webp' },
		pathname: '/',
		schemaTypes: ['SoftwareApplication'],
		title: 'One command. Every tool.',
	}),
	...docs,
];

export default definePresetConfig({
	cards: createCards(pages, (page) => ({
		badge: page.badge,
		description: page.description,
		domain: 'quality.santi020k.com',
		title: page.title,
		variant: 'product',
	}), {
		output: (page) => page.image.output,
		route: (page) => ({
			alt: page.image.alt,
			description: page.description,
			pathname: page.pathname,
			schemaTypes: page.schemaTypes,
			title: page.title,
		}),
	}),
	clean: true,
	outputDirectory: 'public/og/pages',
	preset: {
		brand: { domain: 'quality.santi020k.com', logo: 'public/logo.svg', name: 'quality' },
		decoration: (_data, _context, { accent, theme }) => `
			<g transform="translate(810 196)">
				<rect width="286" height="270" rx="28" fill="${theme.panel}" stroke="${accent}" stroke-opacity="0.72"/>
				<path d="M0 52h286" stroke="${accent}" stroke-opacity="0.42"/>
				<circle cx="26" cy="26" r="5" fill="#fb7185"/><circle cx="46" cy="26" r="5" fill="#fbbf24"/><circle cx="66" cy="26" r="5" fill="#34d399"/>
				<text x="92" y="31" fill="${theme.muted}" font-family="ui-monospace, monospace" font-size="12">quality check</text>
				<g fill="none" stroke-linecap="round" stroke-width="7">
					<path d="m32 94 7 7 14-17" stroke="#34d399"/><path d="M74 94h154" stroke="${theme.muted}"/>
					<path d="m32 139 7 7 14-17" stroke="#34d399"/><path d="M74 139h128" stroke="${theme.muted}"/>
					<path d="m32 184 7 7 14-17" stroke="#34d399"/><path d="M74 184h164" stroke="${theme.muted}"/>
					<path d="m32 229 7 7 14-17" stroke="#34d399"/><path d="M74 229h108" stroke="${theme.muted}"/>
				</g>
			</g>`,
		theme: { accent: '#2563eb', background: '#071329', panel: '#091a35' },
	},
	root,
	routeManifest: { file: 'public/og/manifest.json', publicPath: '/og/pages' },
});
