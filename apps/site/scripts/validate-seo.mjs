import { access, readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import sharp from 'sharp';

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const siteRoot = path.resolve(scriptDirectory, '..');
const outputDirectory = path.join(siteRoot, '.astro-dist');

const collectHtmlFiles = async (directory) => {
	const entries = await readdir(directory, { withFileTypes: true });
	const files = await Promise.all(
		entries.map((entry) => {
			const entryPath = path.join(directory, entry.name);

			return entry.isDirectory() ? collectHtmlFiles(entryPath) : entry.name.endsWith('.html') ? [entryPath] : [];
		}),
	);

	return files.flat();
};

const getAttributes = (tag) =>
	Object.fromEntries([...tag.matchAll(/([:\w-]+)="([^"]*)"/g)].map((match) => [match[1], match[2]]));

const getMeta = (html, key) => {
	for (const tag of html.match(/<meta\b[^>]*>/g) ?? []) {
		const attributes = getAttributes(tag);

		if (attributes.name === key || attributes.property === key) return attributes.content;
	}

	return undefined;
};

const getLink = (html, relation) => {
	for (const tag of html.match(/<link\b[^>]*>/g) ?? []) {
		const attributes = getAttributes(tag);

		if (attributes.rel === relation) return attributes.href;
	}

	return undefined;
};

const requiredMeta = [
	'title',
	'description',
	'author',
	'robots',
	'og:type',
	'og:site_name',
	'og:locale',
	'og:url',
	'og:title',
	'og:description',
	'og:image',
	'og:image:secure_url',
	'og:image:type',
	'og:image:width',
	'og:image:height',
	'og:image:alt',
	'twitter:card',
	'twitter:url',
	'twitter:title',
	'twitter:description',
	'twitter:image',
	'twitter:image:alt',
	'twitter:site',
	'twitter:creator',
];

const htmlFiles = await collectHtmlFiles(outputDirectory);
const socialImages = new Set();

for (const htmlFile of htmlFiles) {
	const html = await readFile(htmlFile, 'utf8');
	const route = path.relative(outputDirectory, htmlFile).replace(/(?:^|\/)index\.html$/, '/').replace(/\.html$/, '/');

	for (const key of requiredMeta) {
		if (!getMeta(html, key)) throw new Error(`${route}: missing ${key} metadata.`);
	}

	if (getMeta(html, 'og:type') === 'article' && !getMeta(html, 'article:section')) {
		throw new Error(`${route}: missing article:section metadata.`);
	}

	if (!getLink(html, 'canonical')) throw new Error(`${route}: missing canonical link.`);
	if (!getLink(html, 'sitemap')) throw new Error(`${route}: missing sitemap link.`);

	const imageUrl = new URL(getMeta(html, 'og:image'));
	const imagePath = path.join(outputDirectory, decodeURIComponent(imageUrl.pathname));

	await access(imagePath);

	const metadata = await sharp(imagePath).metadata();

	if (metadata.width !== 1200 || metadata.height !== 630) {
		throw new Error(`${route}: expected a 1200x630 social image, received ${metadata.width}x${metadata.height}.`);
	}

	if (getMeta(html, 'twitter:image') !== imageUrl.href) {
		throw new Error(`${route}: Open Graph and Twitter images do not match.`);
	}

	socialImages.add(imageUrl.href);
}

if (socialImages.size !== htmlFiles.length) {
	throw new Error(`Expected one unique social image for each of ${htmlFiles.length} pages, found ${socialImages.size}.`);
}

console.log(`Validated complete social metadata and unique 1200x630 images for ${htmlFiles.length} pages.`);
