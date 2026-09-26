import { writable } from 'svelte/store';
import type { contract } from './contract';
import type { Formatter, Arguments } from './format';
import { resolveLocale, type Locale, type Preferences } from './locale';
type Key = keyof typeof contract;
export type Translate = <K extends Key>(
	key: K,
	...args: keyof (typeof contract)[K] extends never ? [] : [Arguments<typeof contract, K>]
) => string;
export type Localization = ReturnType<typeof createLocalization>;
export function createLocalization(format: Formatter<typeof contract>) {
	function view(requested: Locale) {
		const locale = format(requested, 'language.label').locale;
		const t: Translate = (key, ...args) => format(locale, key, ...args).text;
		return { locale, t };
	}
	let preferences: Preferences = {};
	const state = writable(view('en'));
	function publish() {
		state.set(view(resolveLocale(preferences)));
	}
	return {
		subscribe: state.subscribe,
		initialize(hints: Pick<Preferences, 'anonymous' | 'browser' | 'deployment'>) {
			preferences = { ...preferences, ...hints };
			publish();
		},
		deployment(locale?: Locale) {
			preferences.deployment = locale;
			publish();
		},
		account(locale?: Locale) {
			preferences.saved = locale;
			publish();
		},
		select(locale: Locale) {
			preferences.explicit = locale;
			publish();
		}
	};
}
