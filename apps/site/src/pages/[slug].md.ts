import type { APIRoute } from 'astro';
import { getCollection, getEntry } from 'astro:content';

export const prerender = true;

export async function getStaticPaths() {
	const entries = await getCollection('docs');

	return entries.map((entry) => ({ params: { slug: entry.id } }));
}

export const GET: APIRoute = async ({ params }) => {
	const { slug } = params;

	if (!slug) return new Response('Documentation page not found.\n', { status: 404 });

	const entry = await getEntry('docs', slug);

	if (!entry) return new Response('Documentation page not found.\n', { status: 404 });

	const markdown = `# ${entry.data.title}\n\n> ${entry.data.description}\n\n${entry.body?.trim() ?? ''}\n`;

	return new Response(markdown, {
		headers: { 'Content-Type': 'text/markdown; charset=utf-8' },
	});
};
