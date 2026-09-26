import { object, counter } from './admin/catalog-decode';
export type Settings = {
	revision: string;
	logo: boolean;
	background: boolean;
	storage_enabled: boolean;
	bucket: string | null;
};
export type MediaFailure =
	'file' | 'uncertain' | 'unavailable' | 'signed-out' | 'denied' | 'changed' | 'invalid';
export type Result<T> = { kind: 'ready'; value: T } | { kind: 'failed'; code: MediaFailure };
export type MediaApi = {
	settings: () => Promise<Result<Settings>>;
	upload: (path: string, revision: string, file: File) => Promise<Result<string>>;
	remove: (path: string, revision: string) => Promise<Result<string>>;
};
export function validImage(file: File): boolean {
	return (
		['image/png', 'image/jpeg'].includes(file.type) && file.size > 0 && file.size <= 4 * 1024 * 1024
	);
}
export function decodeSettings(v: unknown): Settings | null {
	if (
		!object(v) ||
		!counter(v.revision) ||
		typeof v.logo !== 'boolean' ||
		typeof v.background !== 'boolean' ||
		typeof v.storage_enabled !== 'boolean' ||
		!(v.bucket === null || (typeof v.bucket === 'string' && v.bucket.length <= 63))
	)
		return null;
	return {
		revision: v.revision,
		logo: v.logo,
		background: v.background,
		storage_enabled: v.storage_enabled,
		bucket: v.bucket
	};
}
export function mediaApi(fetcher: typeof fetch): MediaApi {
	async function change(path: string, revision: string, file?: File): Promise<Result<string>> {
		try {
			const r = await fetcher(path, {
				method: file ? 'POST' : 'DELETE',
				credentials: 'same-origin',
				cache: 'no-store',
				headers: {
					'x-darkhorse-csrf': '1',
					'x-darkhorse-revision': revision,
					...(file ? { 'content-type': file.type } : {})
				},
				body: file
			});
			if (!r.ok) return failure(r);
			const data = await r.json();
			return object(data) && counter(data.revision)
				? { kind: 'ready', value: data.revision }
				: uncertain();
		} catch {
			return uncertain();
		}
	}
	return {
		async settings() {
			try {
				const r = await fetcher('/api/admin/branding', {
					credentials: 'same-origin',
					cache: 'no-store'
				});
				if (!r.ok) return failure(r);
				const value = decodeSettings(await r.json());
				return value ? { kind: 'ready', value } : unavailable();
			} catch {
				return unavailable();
			}
		},
		upload: (path, revision, file) =>
			validImage(file)
				? change(path, revision, file)
				: Promise.resolve({ kind: 'failed', code: 'file' }),
		remove: (path, revision) => change(path, revision)
	};
}
function uncertain(): Result<never> {
	return {
		kind: 'failed',
		code: 'uncertain'
	};
}
function unavailable(): Result<never> {
	return {
		kind: 'failed',
		code: 'unavailable'
	};
}
async function failure(response: Response): Promise<Result<never>> {
	if (response.status === 401) return { kind: 'failed', code: 'signed-out' };
	if (response.status === 403)
		return {
			kind: 'failed',
			code: 'denied'
		};
	if (response.status === 409) return { kind: 'failed', code: 'changed' };
	if (response.status === 400 || response.status === 413)
		return {
			kind: 'failed',
			code: 'invalid'
		};
	return uncertain();
}
export async function branding(
	fetcher: typeof fetch
): Promise<{ logo: boolean; background: boolean }> {
	try {
		const r = await fetcher('/api/branding', { credentials: 'omit', cache: 'no-store' });
		if (!r.ok) return { logo: false, background: false };
		const v = await r.json();
		return { logo: v?.logo === true, background: v?.background === true };
	} catch {
		return { logo: false, background: false };
	}
}
