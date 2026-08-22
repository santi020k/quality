import { getCollection } from 'astro:content';

import { renderLlmsFull } from '../lib/llms';

export const prerender = true;

export async function GET() {
	const entries = await getCollection('docs');

	return new Response(renderLlmsFull(entries), {
		headers: { 'Content-Type': 'text/plain; charset=utf-8' },
	});
}
