import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import sharp from 'sharp';

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const siteRoot = path.resolve(scriptDirectory, '..');
const docsDirectory = path.join(siteRoot, 'src', 'content', 'docs');
const outputDirectory = path.join(siteRoot, 'public', 'og', 'pages');
const logo = await readFile(path.join(siteRoot, 'public', 'logo.svg'));
const logoDataUri = `data:image/svg+xml;base64,${logo.toString('base64')}`;

const homePage = {
	description: 'Run trusted code-quality tools across programming ecosystems through one deterministic workflow.',
	pathname: '/',
	title: 'One command. Every tool.',
	type: 'Code quality, orchestrated',
};

const escapeXml = (value) =>
	value
		.replaceAll('&', '&amp;')
		.replaceAll('<', '&lt;')
		.replaceAll('>', '&gt;')
		.replaceAll('"', '&quot;')
		.replaceAll("'", '&apos;');

const readFrontmatterField = (source, field) => {
	const match = source.match(new RegExp(`^${field}:\\s*(.+?)\\s*$`, 'm'));
	const value = match?.[1]?.trim();

	if (!value) throw new Error(`Missing ${field} in documentation frontmatter.`);

	const quote = value.at(0);

	return quote && quote === value.at(-1) && (quote === '"' || quote === "'") ? value.slice(1, -1) : value;
};

const wrapText = (value, maxCharacters, maxLines) => {
	const words = value.split(/\s+/);
	const lines = [];
	let current = '';

	for (const word of words) {
		const candidate = current ? `${current} ${word}` : word;

		if (lines.length === maxLines - 1) {
			current = candidate;
			continue;
		}

		if (candidate.length <= maxCharacters || current.length === 0) {
			current = candidate;
			continue;
		}

		lines.push(current);
		current = word;
	}

	if (current && lines.length < maxLines) lines.push(current);

	return lines;
};

const getFileName = (pathname) => {
	const slug = pathname.replace(/^\/+|\/+$/g, '').split('/').filter(Boolean).join('--');

	return `${slug || 'index'}.webp`;
};

const textLines = (lines, { fill, fontSize, fontWeight, lineHeight, x, y }) =>
	lines
		.map(
			(line, index) =>
				`<text x="${x}" y="${y + index * lineHeight}" fill="${fill}" font-family="Inter, ui-sans-serif, system-ui, sans-serif" font-size="${fontSize}" font-weight="${fontWeight}">${escapeXml(line)}</text>`,
		)
		.join('\n');

const createCard = ({ description, pathname, title, type }) => {
	const titleLines = wrapText(title, 18, 2);
	const descriptionLines = wrapText(description, 66, 2);
	const titleSize = title.length > 34 ? 58 : 68;

	return `
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 630" role="img" aria-label="${escapeXml(`${title} — ${description}`)}">
	<defs>
		<linearGradient id="background" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#071329"/><stop offset="1" stop-color="#0b1f3f"/></linearGradient>
		<linearGradient id="brand" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#2563eb"/><stop offset="1" stop-color="#0ea5e9"/></linearGradient>
		<radialGradient id="glow" cx="0" cy="0" r="1" gradientTransform="translate(1030 90) rotate(135) scale(660 520)"><stop stop-color="#2563eb" stop-opacity=".45"/><stop offset="1" stop-color="#071329" stop-opacity="0"/></radialGradient>
		<pattern id="grid" width="48" height="48" patternUnits="userSpaceOnUse"><path d="M48 0H0v48" fill="none" stroke="#93c5fd" stroke-opacity=".07"/></pattern>
		<filter id="shadow" x="-60%" y="-60%" width="220%" height="240%"><feDropShadow dx="0" dy="16" stdDeviation="18" flood-color="#020617" flood-opacity=".72"/></filter>
	</defs>
	<rect width="1200" height="630" fill="url(#background)"/>
	<rect width="1200" height="630" fill="url(#glow)"/>
	<rect width="1200" height="630" fill="url(#grid)"/>
	<rect x="26" y="26" width="1148" height="578" rx="34" fill="none" stroke="#dbeafe" stroke-opacity=".12"/>
	<image href="${logoDataUri}" x="72" y="62" width="70" height="70" filter="url(#shadow)"/>
	<text x="164" y="99" fill="#f8fafc" font-family="Inter, ui-sans-serif, system-ui, sans-serif" font-size="28" font-weight="750">quality</text>
	<text x="164" y="127" fill="#93a4bf" font-family="Inter, ui-sans-serif, system-ui, sans-serif" font-size="15">quality-cli.santi020k.chatgpt.site</text>
	<g transform="translate(72 178)"><rect width="${Math.min(390, type.length * 10 + 54)}" height="38" rx="19" fill="#132b50" stroke="#3b82f6" stroke-opacity=".72"/><circle cx="21" cy="19" r="5" fill="#38bdf8"/><text x="38" y="25" fill="#dbeafe" font-family="Inter, ui-sans-serif, system-ui, sans-serif" font-size="13" font-weight="750" letter-spacing="1.2">${escapeXml(type.toUpperCase())}</text></g>
	${textLines(titleLines, { fill: '#f8fafc', fontSize: titleSize, fontWeight: 780, lineHeight: 76, x: 72, y: 306 })}
	${textLines(descriptionLines, { fill: '#b8c6dc', fontSize: 20, fontWeight: 450, lineHeight: 30, x: 74, y: 494 })}
	<g transform="translate(845 214)" filter="url(#shadow)">
		<rect width="274" height="260" rx="24" fill="#091a35" stroke="#3b82f6" stroke-opacity=".72"/>
		<path d="M0 48h274" stroke="#3b82f6" stroke-opacity=".42"/>
		<circle cx="24" cy="24" r="5" fill="#fb7185"/><circle cx="43" cy="24" r="5" fill="#fbbf24"/><circle cx="62" cy="24" r="5" fill="#34d399"/>
		<text x="86" y="29" fill="#8293ae" font-family="ui-monospace, monospace" font-size="11">quality check</text>
		<g fill="none" stroke-linecap="round" stroke-width="7"><path d="m28 86 7 7 13-16" stroke="#34d399"/><path d="M66 86h148" stroke="#7f91ad"/><path d="m28 128 7 7 13-16" stroke="#34d399"/><path d="M66 128h123" stroke="#7f91ad"/><path d="m28 170 7 7 13-16" stroke="#34d399"/><path d="M66 170h158" stroke="#7f91ad"/><path d="m28 212 7 7 13-16" stroke="#34d399"/><path d="M66 212h104" stroke="#7f91ad"/></g>
	</g>
	<text x="74" y="574" fill="#7185a4" font-family="ui-monospace, monospace" font-size="15">${escapeXml(pathname)}</text>
</svg>`.trim();
};

const docs = await Promise.all(
	(await readdir(docsDirectory))
		.filter((fileName) => fileName.endsWith('.md') || fileName.endsWith('.mdx'))
		.sort()
		.map(async (fileName) => {
			const source = await readFile(path.join(docsDirectory, fileName), 'utf8');
			const slug = fileName.replace(/\.mdx?$/, '');

			return {
				description: readFrontmatterField(source, 'description'),
				pathname: `/${slug}/`,
				title: readFrontmatterField(source, 'title'),
				type: 'Documentation',
			};
		}),
);

const pages = [homePage, ...docs];

await mkdir(outputDirectory, { recursive: true });

let writes = 0;

for (const page of pages) {
	const outputPath = path.join(outputDirectory, getFileName(page.pathname));
	const image = await sharp(Buffer.from(createCard(page))).webp({ effort: 4, quality: 86 }).toBuffer();
	const current = await readFile(outputPath).catch(() => undefined);

	if (!current?.equals(image)) {
		await writeFile(outputPath, image);
		writes += 1;
	}
}

console.log(`Generated ${pages.length} quality Open Graph images (1200×630); wrote ${writes}.`);
