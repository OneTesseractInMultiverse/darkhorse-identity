import { describe, expect, it, vi } from 'vitest';
import {
	MAX_REFERENCE_BYTES,
	filterOperations,
	filterRouteEntries,
	localizeOperation,
	loadReference,
	operationsFromDocument,
	resolveLocalSchema,
	type ApiDocument,
	type RouteClassification
} from '../../../src/lib/api-reference';

const document: ApiDocument = {
	openapi: '3.2.1',
	info: { version: '0.1.0', title: 'Protocol API' },
	paths: {
		'/token': {
			post: {
				operationId: 'postToken',
				summary: 'Exchange a code',
				description: 'Use verified TLS.',
				tags: ['Tokens'],
				'x-darkhorse-localization': {
					es: {
						summary: 'Canjear el código de autorización',
						description: 'Usa el backend autorizado.',
						group: 'Tokens'
					}
				},
				responses: { '200': { description: 'Success' } }
			}
		},
		'/authorize': {
			get: {
				operationId: 'getAuthorization',
				summary: 'Start browser sign-in',
				description: 'Use PKCE.',
				tags: ['Authorization'],
				'x-darkhorse-localization': {
					es: {
						summary: 'Iniciar la autorización',
						description: 'Usa PKCE y una redirección exacta.',
						group: 'Autorización'
					}
				},
				responses: { '302': { description: 'Redirect' } }
			}
		}
	}
};

describe('API documentation reader model', () => {
	it('builds deterministic method/path operations and bounded literal search', () => {
		const operations = operationsFromDocument(document);
		expect(operations.map(({ operationId }) => operationId)).toEqual([
			'getAuthorization',
			'postToken'
		]);
		expect(filterOperations(operations, 'PKCE')).toEqual([operations[0]]);
		expect(filterOperations(operations, 'post')).toEqual([operations[1]]);
		expect(filterOperations(operations, 'missing')).toEqual([]);
	});

	it('localizes protocol guidance without changing routes or removing English search terms', () => {
		const source = operationsFromDocument(document).find(
			(operation) => operation.operationId === 'postToken'
		)!;
		const spanish = localizeOperation(source, 'es');

		expect(spanish.summary).toBe('Canjear el código de autorización');
		expect(spanish.description).toBe('Usa el backend autorizado.');
		expect(spanish.tags).toEqual(['Tokens']);
		expect(spanish.path).toBe(source.path);
		expect(spanish.operationId).toBe(source.operationId);
		expect(spanish.value).toBe(source.value);
		expect(filterOperations([spanish], 'verified TLS')).toEqual([spanish]);
		expect(filterOperations([spanish], 'autorización')).toEqual([spanish]);
	});

	it('searches the bounded route inventory by path, method, purpose and localized surface name', () => {
		const entries: RouteClassification['entries'] = [
			{
				source: 'authentication_http.rs',
				function: 'router',
				method: 'GET',
				path: '/api/admin/users/{id}',
				handler: 'detail',
				surface: 'first_party_browser',
				rationale: 'First-party user administration.'
			},
			{
				source: 'http.rs',
				function: 'router',
				method: 'GET',
				path: '/health/live',
				handler: 'liveness',
				surface: 'operational',
				rationale: 'Infrastructure health probe.'
			}
		];

		expect(filterRouteEntries(entries, '  /API/ADMIN/USERS  ')).toEqual([entries[0]]);
		expect(
			filterRouteEntries(entries, 'get', (surface) =>
				surface === 'operational' ? 'Operaciones' : 'Administración'
			)
		).toEqual(entries);
		expect(
			filterRouteEntries(entries, 'administración', (surface) =>
				surface === 'operational' ? 'Operaciones' : 'Administración'
			)
		).toEqual([entries[0]]);
		expect(filterRouteEntries(entries, 'user administration')).toEqual([entries[0]]);
		expect(filterRouteEntries(entries, 'not present')).toEqual([]);
		expect(entries).toHaveLength(2);
	});

	it('caps route search input and result projection', () => {
		const entries: RouteClassification['entries'] = Array.from({ length: 600 }, (_, index) => ({
			source: 'routes.rs',
			function: 'router',
			method: 'GET',
			path: `/route/${index}`,
			handler: 'read',
			surface: 'first_party_browser',
			rationale: 'Registered route.'
		}));
		expect(filterRouteEntries(entries, '')).toHaveLength(512);
	});

	it('expands local schema references with a cycle and work bound', () => {
		const schema = {
			components: {
				schemas: { Token: { type: 'object', properties: { token: { type: 'string' } } } }
			},
			value: { $ref: '#/components/schemas/Token' }
		};
		expect(resolveLocalSchema(schema.value, schema)).toEqual(schema.components.schemas.Token);
		const cycle = { a: { $ref: '#/b' }, b: { $ref: '#/a' } };
		expect(JSON.stringify(resolveLocalSchema(cycle.a, cycle))).toContain('Circular');
		expect(JSON.stringify(resolveLocalSchema({ nested: { value: 1 } }, schema, 1))).toContain(
			'limit'
		);
	});

	it('loads only fixed same-origin static inputs and rejects oversized or malformed documents', async () => {
		const fetcher = vi
			.fn<typeof fetch>()
			.mockResolvedValueOnce(new Response(JSON.stringify(document)))
			.mockResolvedValueOnce(
				new Response(
					JSON.stringify({
						schema: 1,
						version: '0.1.0',
						entries: [
							{
								source: 'tokens.rs',
								function: 'router',
								method: 'GET',
								path: '/authorize',
								handler: 'authorize',
								surface: 'integration',
								operation_id: 'getAuthorization',
								rationale: 'contract'
							},
							{
								source: 'tokens.rs',
								function: 'router',
								method: 'POST',
								path: '/token',
								handler: 'token',
								surface: 'integration',
								operation_id: 'postToken',
								rationale: 'contract'
							}
						]
					})
				)
			);
		const result = await loadReference(fetcher);
		expect(result.kind).toBe('ready');
		expect(fetcher).toHaveBeenNthCalledWith(1, '/reference/openapi-v1.json', {
			credentials: 'omit',
			cache: 'no-store',
			redirect: 'error'
		});
		expect(fetcher).toHaveBeenNthCalledWith(2, '/reference/route-classification-v1.json', {
			credentials: 'omit',
			cache: 'no-store',
			redirect: 'error'
		});
		expect(fetcher).toHaveBeenCalledTimes(2);
		const tooLarge = new Response('x'.repeat(MAX_REFERENCE_BYTES + 1));
		expect((await loadReference(vi.fn<typeof fetch>().mockResolvedValue(tooLarge))).kind).toBe(
			'unavailable'
		);
		expect(
			(await loadReference(vi.fn<typeof fetch>().mockResolvedValue(new Response('<script>')))).kind
		).toBe('unavailable');
		const mismatch = vi
			.fn<typeof fetch>()
			.mockResolvedValueOnce(new Response(JSON.stringify(document)))
			.mockResolvedValueOnce(
				new Response(JSON.stringify({ schema: 1, version: '0.1.0', entries: [] }))
			);
		expect((await loadReference(mismatch)).kind).toBe('unavailable');
		const rejected = vi.fn<typeof fetch>().mockRejectedValue(new Error('hidden detail'));
		expect(await loadReference(rejected)).toEqual({ kind: 'unavailable' });
	});
});
