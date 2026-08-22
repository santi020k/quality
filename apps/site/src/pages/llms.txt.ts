import { getCollection } from 'astro:content';

import { renderLlmsIndex } from '../lib/llms';

export const prerender = true;

export async function GET({ site }: { site: URL }) {
	const entries = await getCollection('docs');

	return new Response(renderLlmsIndex(site, entries), {
		headers: { 'Content-Type': 'text/plain; charset=utf-8' },
	});
}
