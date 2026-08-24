import { standardAuditRules } from '@santi020k/og/audit/rules';
import { defineAuditConfig } from '@santi020k/og/audit/config';

const siteUrl = 'https://quality.santi020k.com';

export default defineAuditConfig({
	directory: '.astro-dist',
	manifest: '.astro-dist/og/manifest.json',
	requireUniqueImages: true,
	root: '.',
	siteUrl,
	...standardAuditRules({
		alternates: false,
		llms: {
			compatibilityFiles: ['llm.txt'],
			severity: 'error',
		},
		redirects: false,
		robots: {
			expectedSitemaps: [new URL('/sitemap.xml', siteUrl).href],
		},
		sitemap: { reportOrphans: true, severity: 'error' },
	}),
});
