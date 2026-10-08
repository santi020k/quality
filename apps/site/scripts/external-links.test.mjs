import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createSatteriMarkdownProcessor } from '@astrojs/markdown-satteri';
import externalLinks from './external-links.mjs';

const processor = await createSatteriMarkdownProcessor({ hastPlugins: [externalLinks()] });

test('external documentation links open safely in generated HTML', async () => {
	const { code } = await processor.render('[Source](https://github.com/santi020k/quality).');
	assert.match(code, /target="_blank"/);
	assert.match(code, /rel="noopener noreferrer"/);
});

test('routes, same-origin URLs, fragments and non-web links stay in the same tab', async () => {
	const { code } = await processor.render('[Commands](/commands/) [Heading](#output) [Email](mailto:hello@example.com) [Docs](https://quality.santi020k.com/overview/).');
	assert.doesNotMatch(code, /target=|rel=/);
	assert.match(code, /href="\/commands\/"/);
	assert.match(code, /href="#output"/);
});

test('links nested in lists and tables receive the same protection', async () => {
	const { code } = await processor.render('- [Source](https://github.com/santi020k/quality)\n\n| Tool |\n| --- |\n| [Lumen](https://lumen.santi020k.com) |');
	assert.equal(code.match(/target="_blank"/g)?.length, 2);
	assert.equal(code.match(/rel="noopener noreferrer"/g)?.length, 2);
});

test('applying the policy twice does not duplicate relationship tokens', async () => {
	const repeated = await createSatteriMarkdownProcessor({ hastPlugins: [externalLinks(), externalLinks()] });
	const { code } = await repeated.render('[Source](https://github.com/santi020k/quality)');
	assert.equal(code.match(/noopener/g)?.length, 1);
	assert.equal(code.match(/noreferrer/g)?.length, 1);
});
