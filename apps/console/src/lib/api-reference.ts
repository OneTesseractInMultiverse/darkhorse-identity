export const MAX_REFERENCE_BYTES = 2 * 1024 * 1024;
const METHODS = new Set(['get', 'post', 'put', 'patch', 'delete']);
const API_PATH = '/reference/openapi-v1.json';
const CLASSIFICATION_PATH = '/reference/route-classification-v1.json';

export interface ApiOperation {
	path: string;
	method: string;
	operationId: string;
	summary: string;
	description: string;
	tags: string[];
	value: Record<string, unknown>;
}

export interface ApiDocument {
	openapi: string;
	info: { title: string; version: string; description?: string };
	paths: Record<string, Record<string, unknown>>;
	components?: Record<string, unknown>;
}

export interface RouteClassification {
	schema: number;
	version: string;
	entries: Array<{
		source: string;
		function: string;
		method: string;
		path: string;
		handler: string;
		surface: string;
		operation_id?: string;
		rationale: string;
	}>;
}

export type RouteEntry = RouteClassification['entries'][number];

export type ReferenceState =
	| { kind: 'ready'; document: ApiDocument; classification: RouteClassification }
	| { kind: 'unavailable' };

export async function loadReference(fetcher: typeof fetch): Promise<ReferenceState> {
	try {
		const [documentResponse, classificationResponse] = await Promise.all([
			fetcher(API_PATH, { credentials: 'omit', cache: 'no-store', redirect: 'error' }),
			fetcher(CLASSIFICATION_PATH, { credentials: 'omit', cache: 'no-store', redirect: 'error' })
		]);
		const [documentValue, classificationValue] = await Promise.all([
			readJson(documentResponse),
			readJson(classificationResponse)
		]);
		if (
			!isApiDocument(documentValue) ||
			!isRouteClassification(classificationValue) ||
			documentValue.info.version !== classificationValue.version ||
			!routesMatch(documentValue, classificationValue)
		)
			return { kind: 'unavailable' };
		return { kind: 'ready', document: documentValue, classification: classificationValue };
	} catch {
		return { kind: 'unavailable' };
	}
}

function routesMatch(document: ApiDocument, classification: RouteClassification): boolean {
	const documented = operationsFromDocument(document);
	const integrations = classification.entries.filter((entry) => entry.surface === 'integration');
	if (documented.length !== integrations.length || documented.length > 512) return false;
	const routes = new Map(
		integrations.map((entry) => [`${entry.method.toUpperCase()} ${entry.path}`, entry.operation_id])
	);
	if (routes.size !== integrations.length) return false;
	const operationIds = new Set<string>();
	for (const operation of documented) {
		const key = `${operation.method} ${operation.path}`;
		if (!operation.operationId || operationIds.has(operation.operationId)) return false;
		operationIds.add(operation.operationId);
		if (routes.get(key) !== operation.operationId) return false;
	}
	return true;
}

export function operationsFromDocument(document: ApiDocument): ApiOperation[] {
	const operations: ApiOperation[] = [];
	for (const [path, pathItem] of Object.entries(document.paths)) {
		for (const [method, value] of Object.entries(pathItem)) {
			if (!METHODS.has(method) || !isRecord(value)) continue;
			operations.push({
				path,
				method: method.toUpperCase(),
				operationId: stringField(value.operationId),
				summary: stringField(value.summary),
				description: stringField(value.description),
				tags: Array.isArray(value.tags) ? value.tags.filter(isString) : [],
				value
			});
		}
	}
	return operations.sort(
		(left, right) => left.path.localeCompare(right.path) || left.method.localeCompare(right.method)
	);
}

export function filterOperations(operations: ApiOperation[], query: string): ApiOperation[] {
	const term = query.slice(0, 128).trim().toLocaleLowerCase();
	const found = term
		? operations.filter((operation) =>
				[
					operation.path,
					operation.method,
					operation.operationId,
					operation.summary,
					operation.description,
					...operation.tags
				]
					.join(' ')
					.toLocaleLowerCase()
					.includes(term)
			)
		: operations;
	return found.slice(0, 100);
}

/** Search the static route inventory locally with bounded input and output work. */
export function filterRouteEntries(
	entries: RouteEntry[],
	query: string,
	surfaceName: (surface: string) => string = (surface) => surface
): RouteEntry[] {
	const term = query.slice(0, 128).trim().toLocaleLowerCase();
	const found = term
		? entries.filter((entry) =>
				[
					entry.method,
					entry.path,
					entry.surface,
					surfaceName(entry.surface),
					entry.operation_id ?? '',
					entry.rationale
				]
					.join(' ')
					.toLocaleLowerCase()
					.includes(term)
			)
		: entries;
	return found.slice(0, 512);
}

/** Dereference only local pointers; cap work and recursion for safe rendering. */
export function resolveLocalSchema(
	value: unknown,
	document: unknown,
	maxDepth = 12,
	maxNodes = 2048
): unknown {
	const budget = { remaining: Math.min(Math.max(maxNodes, 1), 4096) };
	return resolve(value, document, 0, maxDepth, budget, new Set());
}

async function readJson(response: Response): Promise<unknown> {
	if (!response.ok || !response.body) throw new Error('Reference unavailable.');
	const length = response.headers.get('content-length');
	if (length && /^\d+$/.test(length) && Number(length) > MAX_REFERENCE_BYTES)
		throw new Error('Reference exceeds its size limit.');
	const reader = response.body.getReader();
	const chunks: Uint8Array[] = [];
	let size = 0;
	try {
		while (true) {
			const { done, value } = await reader.read();
			if (done) break;
			size += value.byteLength;
			if (size > MAX_REFERENCE_BYTES) {
				await reader.cancel();
				throw new Error('Reference exceeds its size limit.');
			}
			chunks.push(value);
		}
	} finally {
		reader.releaseLock();
	}
	const bytes = new Uint8Array(size);
	let offset = 0;
	for (const chunk of chunks) {
		bytes.set(chunk, offset);
		offset += chunk.byteLength;
	}
	return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)) as unknown;
}

function resolve(
	value: unknown,
	document: unknown,
	depth: number,
	maxDepth: number,
	budget: { remaining: number },
	activeRefs: Set<string>
): unknown {
	budget.remaining--;
	if (budget.remaining < 0) return '[Reference display limit reached]';
	if (depth > maxDepth) return '[Reference depth limit reached]';
	if (Array.isArray(value))
		return value.map((item) => resolve(item, document, depth + 1, maxDepth, budget, activeRefs));
	if (!isRecord(value)) return value;
	if (typeof value.$ref === 'string') {
		const reference = value.$ref;
		if (!reference.startsWith('#/')) return { notice: 'External references are not loaded.' };
		if (activeRefs.has(reference)) return { notice: 'Circular reference omitted.' };
		const target = pointer(document, reference);
		if (target === undefined) return { notice: 'Unresolved local reference.' };
		const nextRefs = new Set(activeRefs);
		nextRefs.add(reference);
		return resolve(target, document, depth + 1, maxDepth, budget, nextRefs);
	}
	const result: Record<string, unknown> = {};
	for (const [key, child] of Object.entries(value)) {
		if (key === '__proto__' || key === 'constructor' || key === 'prototype') continue;
		result[key] = resolve(child, document, depth + 1, maxDepth, budget, activeRefs);
	}
	return result;
}

function pointer(root: unknown, reference: string): unknown {
	return reference
		.slice(2)
		.split('/')
		.map((segment) => segment.replace(/~1/g, '/').replace(/~0/g, '~'))
		.reduce<unknown>((current, segment) => {
			if (Array.isArray(current) && /^(0|[1-9]\d*)$/.test(segment)) return current[Number(segment)];
			return isRecord(current) ? current[segment] : undefined;
		}, root);
}

function isApiDocument(value: unknown): value is ApiDocument {
	return (
		isRecord(value) &&
		value.openapi === '3.2.1' &&
		isRecord(value.info) &&
		isString(value.info.title) &&
		isString(value.info.version) &&
		value.info.version.length <= 64 &&
		isRecord(value.paths) &&
		Object.keys(value.paths).length <= 512
	);
}

function isRouteClassification(value: unknown): value is RouteClassification {
	return (
		isRecord(value) &&
		value.schema === 1 &&
		isString(value.version) &&
		Array.isArray(value.entries) &&
		value.entries.length <= 1024 &&
		value.entries.every(
			(entry) =>
				isRecord(entry) &&
				isString(entry.source) &&
				isString(entry.function) &&
				isString(entry.method) &&
				isString(entry.path) &&
				isString(entry.handler) &&
				isString(entry.surface) &&
				isString(entry.rationale)
		)
	);
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function isString(value: unknown): value is string {
	return typeof value === 'string' && value.length <= 4096;
}
function stringField(value: unknown): string {
	return isString(value) ? value : '';
}
