import type { Locale } from './locale';
export async function presentation(fetcher: typeof fetch): Promise<Locale | undefined> {
	try {
		const response = await fetcher('/api/presentation', {
			credentials: 'omit',
			cache: 'no-store',
			redirect: 'error'
		});
		if (!response.ok) return;
		const body = await response.json();
		return body?.default_locale === 'en' || body?.default_locale === 'es'
			? body.default_locale
			: undefined;
	} catch {
		return;
	}
}
