import type { Locale } from './i18n/locale';
import { object, reference, counter } from './admin/catalog-decode';
export type Fields = {
	first_name: string;
	second_name: string;
	last_name: string;
	second_last_name: string;
	country: string;
	calling_code: string;
	national_number: string;
	bio: string;
};
export type Profile = Fields & {
	preferred_locale: Locale | null;
	id: string;
	revision: string;
	email: string;
	active: boolean;
	email_verified: boolean;
};
export type Options = {
	country_version: string;
	phone_version: string;
	countries: { code: string; name: string }[];
	calling_codes: string[];
};
export type Failure = {
	kind: 'signed-out' | 'denied' | 'recent' | 'changed' | 'invalid' | 'uncertain' | 'unavailable';
};
export type Result<T> = { kind: 'ready'; value: T } | Failure;
export type ProfileApi = {
	language: (revision: string, locale: Locale | null) => Promise<Result<Profile>>;
	load: (target?: string) => Promise<Result<Profile>>;
	options: () => Promise<Result<Options>>;
	save: (target: string, revision: string, fields: Fields) => Promise<Result<Profile>>;
};
function text(v: unknown, max: number): v is string {
	return typeof v === 'string' && [...v].length <= max;
}
export function fields(p: Fields): Fields {
	return {
		first_name: p.first_name,
		second_name: p.second_name,
		last_name: p.last_name,
		second_last_name: p.second_last_name,
		country: p.country,
		calling_code: p.calling_code,
		national_number: p.national_number,
		bio: p.bio
	};
}
export function decodeProfile(v: unknown): Profile | null {
	if (
		!object(v) ||
		!reference(v.id) ||
		!counter(v.revision) ||
		!text(v.email, 254) ||
		typeof v.active !== 'boolean' ||
		typeof v.email_verified !== 'boolean' ||
		!text(v.first_name, 100) ||
		!text(v.last_name, 100) ||
		!text(v.second_name, 100) ||
		!text(v.second_last_name, 100) ||
		!text(v.country, 2) ||
		!text(v.calling_code, 3) ||
		!text(v.national_number, 14) ||
		!text(v.bio, 2000) ||
		(v.preferred_locale !== undefined &&
			v.preferred_locale !== null &&
			v.preferred_locale !== 'en' &&
			v.preferred_locale !== 'es')
	)
		return null;
	return {
		id: v.id,
		preferred_locale: v.preferred_locale ?? null,
		revision: v.revision,
		email: v.email,
		active: v.active,
		email_verified: v.email_verified,
		first_name: v.first_name,
		second_name: v.second_name,
		last_name: v.last_name,
		second_last_name: v.second_last_name,
		country: v.country,
		calling_code: v.calling_code,
		national_number: v.national_number,
		bio: v.bio
	};
}
export function decodeOptions(v: unknown): Options | null {
	if (
		!object(v) ||
		!text(v.country_version, 100) ||
		!text(v.phone_version, 100) ||
		!Array.isArray(v.countries) ||
		v.countries.length > 300 ||
		!Array.isArray(v.calling_codes) ||
		v.calling_codes.length > 300
	)
		return null;
	const countries: { code: string; name: string }[] = [];
	for (const c of v.countries) {
		if (!object(c) || !text(c.code, 2) || !/^[A-Z]{2}$/.test(c.code) || !text(c.name, 100))
			return null;
		countries.push({ code: c.code, name: c.name });
	}
	const codes: string[] = [];
	for (const c of v.calling_codes) {
		if (typeof c !== 'string' || !/^[1-9][0-9]{0,2}$/.test(c)) return null;
		codes.push(c);
	}
	if (
		new Set(countries.map((c) => c.code)).size !== countries.length ||
		new Set(codes).size !== codes.length
	)
		return null;
	return {
		country_version: v.country_version,
		phone_version: v.phone_version,
		countries,
		calling_codes: codes
	};
}
export function profileApi(fetcher: typeof fetch): ProfileApi {
	async function read<T>(path: string, decode: (v: unknown) => T | null): Promise<Result<T>> {
		try {
			const r = await fetcher(`/api/profiles/${path}`, {
				credentials: 'same-origin',
				cache: 'no-store'
			});
			if (!r.ok) return failure(r);
			const value = decode(await r.json());
			return value ? { kind: 'ready', value } : { kind: 'unavailable' };
		} catch {
			return { kind: 'unavailable' };
		}
	}
	async function write(path: string, body: unknown): Promise<Result<Profile>> {
		try {
			const r = await fetcher(`/api/profiles/${path}`, {
				method: 'POST',
				credentials: 'same-origin',
				cache: 'no-store',
				headers: { 'content-type': 'application/json', 'x-darkhorse-csrf': '1' },
				body: JSON.stringify(body)
			});
			if (!r.ok) return failure(r);
			const p = decodeProfile(await r.json());
			return p ? { kind: 'ready', value: p } : { kind: 'uncertain' };
		} catch {
			return { kind: 'uncertain' };
		}
	}
	return {
		load: (target = 'me') => read(encodeURIComponent(target), decodeProfile),
		options: () => read('options', decodeOptions),
		save: (target, revision, value) =>
			write(encodeURIComponent(target), { revision, ...fields(value) }),
		language: (revision, locale) => write('me/language', { revision, locale })
	};
}
async function failure(r: Response): Promise<Failure> {
	if (r.status === 401) return { kind: 'signed-out' };
	if (r.status === 403) {
		try {
			if ((await r.json())?.error === 'recent_authentication_required') return { kind: 'recent' };
		} catch {
			/* Fixed denial below. */
		}
		return { kind: 'denied' };
	}
	if (r.status === 409) return { kind: 'changed' };
	if (r.status === 400) return { kind: 'invalid' };
	return { kind: 'uncertain' };
}
export function message(error: Failure): string {
	switch (error.kind) {
		case 'signed-out':
			return 'Sign in to view this profile.';
		case 'denied':
			return 'You do not have access to this profile.';
		case 'recent':
			return 'Sign out and sign in again before editing. A sign-in within five minutes is required.';
		case 'changed':
			return 'This profile changed. Reload before editing again.';
		case 'invalid':
			return 'Check the profile values and phone number, then reload before editing again.';
		case 'uncertain':
			return 'The change could not be confirmed. Reload the profile before making another change.';
		case 'unavailable':
			return 'The profile is temporarily unavailable. Try reloading.';
	}
}
