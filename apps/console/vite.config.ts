import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vitest/config';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { compileCatalog, packCatalog } from './src/lib/i18n/catalog.ts';
import { contract } from './src/lib/i18n/contract.ts';
import { adminContract } from './src/lib/i18n/admin-contract.ts';
import en from './src/lib/i18n/catalogs/en.json' with { type: 'json' };
import es from './src/lib/i18n/catalogs/es.json' with { type: 'json' };
import adminEn from './src/lib/i18n/catalogs/admin-en.json' with { type: 'json' };
import adminEs from './src/lib/i18n/catalogs/admin-es.json' with { type: 'json' };

const bundledCatalogs = [
	{ path: '/src/lib/i18n/catalogs/en.json', contract, value: en, locale: 'en' },
	{ path: '/src/lib/i18n/catalogs/es.json', contract, value: es, locale: 'es' },
	{
		path: '/src/lib/i18n/catalogs/admin-en.json',
		contract: adminContract,
		value: adminEn,
		locale: 'en'
	},
	{
		path: '/src/lib/i18n/catalogs/admin-es.json',
		contract: adminContract,
		value: adminEs,
		locale: 'es'
	}
] as const;

export default defineConfig({
	plugins: [
		{
			name: 'darkhorse-catalogs',
			apply: 'build',
			enforce: 'post',
			buildStart() {
				compileCatalog(contract, en, 'en');
				compileCatalog(contract, es, 'es');
				compileCatalog(adminContract, adminEn, 'en');
				compileCatalog(adminContract, adminEs, 'es');
			},
			transform(_source, id) {
				const catalog = bundledCatalogs.find((entry) =>
					id.replaceAll('\\', '/').endsWith(entry.path)
				);
				if (!catalog) return;
				return {
					code: `export default ${JSON.stringify(packCatalog(catalog.contract, catalog.value, catalog.locale))};`,
					map: null
				};
			}
		},
		tailwindcss(),
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			adapter: adapter(),
			csp: {
				mode: 'hash',
				directives: {
					'default-src': ['self'],
					'script-src': ['self'],
					'style-src': ['self', 'unsafe-inline'],
					'img-src': ['self', 'data:'],
					'object-src': ['none'],
					'base-uri': ['none'],
					'form-action': ['self']
				}
			}
		})
	],
	server: {
		host: '127.0.0.1',
		port: 5173,
		strictPort: true,
		hmr: { protocol: 'wss', host: 'localhost', clientPort: 8443 }
	},
	resolve: process.env.VITEST ? { conditions: ['browser'] } : undefined,
	test: {
		environment: 'jsdom',
		include: ['tests/unit/**/*.test.ts'],
		setupFiles: ['tests/setup.ts'],
		coverage: {
			provider: 'v8',
			include: ['src/lib/**/*.ts', 'src/lib/**/*.svelte', 'src/routes/**/+page.svelte'],
			exclude: ['src/lib/index.ts', 'src/lib/components/ui/**'],
			reporter: ['text', 'html', 'json-summary']
		}
	}
});
