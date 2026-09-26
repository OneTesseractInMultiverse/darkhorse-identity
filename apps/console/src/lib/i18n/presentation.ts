import type { Locale } from './locale';
const translatedRoutes = [
	'/',
	'/account/profile',
	'/security/sessions',
	'/security/keys',
	'/security/email',
	'/invitation',
	'/authorization',
	'/console/settings',
	'/console/users',
	'/console/applications',
	'/console/clients',
	'/console/resources',
	'/console/scopes',
	'/console/roles',
	'/console/capabilities',
	'/console/profile'
];

/** Metadata remains left-to-right; right-to-left support is not enabled. */
export function documentPresentation(pathname: string, selected: Locale): [Locale, 'ltr'] {
	return [translatedRoutes.includes(pathname) ? selected : 'en', 'ltr'];
}

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
