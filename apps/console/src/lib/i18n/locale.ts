/** Presentation policy only. External syntax belongs in input.ts. */
export const locales = ['en', 'es'] as const;
export type Locale = (typeof locales)[number];
export type Preferences = {
	explicit?: Locale;
	saved?: Locale;
	anonymous?: Locale;
	transaction?: readonly Locale[];
	browser?: readonly Locale[];
	deployment?: Locale;
};
export function resolveLocale(p: Preferences): Locale {
	return (
		p.explicit ??
		p.saved ??
		p.anonymous ??
		p.transaction?.[0] ??
		p.browser?.[0] ??
		p.deployment ??
		'en'
	);
}
