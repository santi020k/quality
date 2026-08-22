import { getCollection } from 'astro:content';

export const prerender = true;

const escapeXml = (value: string) =>
	value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;').replaceAll("'", '&apos;');

export async function GET({ site }: { site: URL }) {
	const entries = await getCollection('docs');
	const paths = ['/', ...entries.map((entry) => `/${entry.id}/`)];
	const urls = paths
		.sort()
		.map((path) => `  <url><loc>${escapeXml(new URL(path, site).href)}</loc></url>`)
		.join('\n');

	return new Response(`<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`, {
		headers: { 'Content-Type': 'application/xml; charset=utf-8' },
	});
}
