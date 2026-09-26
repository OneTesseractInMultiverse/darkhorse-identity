import type { Locale } from './locale';
const key = 'darkhorse.locale.v1';
/** Browser-only preference; never includes identity or authentication material. */
export function readPreference(storage: Pick<Storage, 'getItem'>): Locale | undefined {
	try {
		const value = storage.getItem(key);
		return value === 'en' || value === 'es' ? value : undefined;
	} catch {
		return;
	}
}
export function writePreference(storage: Pick<Storage, 'setItem'>, locale: Locale): boolean {
	try {
		storage.setItem(key, locale);
		return true;
	} catch {
		return false;
	}
}
export function browserStorage(): Pick<Storage, 'getItem' | 'setItem'> {
	// Access to the storage property itself may throw under browser privacy settings.
	return {
		getItem: (key) => window.localStorage.getItem(key),
		setItem: (key, value) => window.localStorage.setItem(key, value)
	};
}
