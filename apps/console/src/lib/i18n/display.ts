import type { Locale } from './locale';
type Country = { code: string; name: string };
export function countries(locale: Locale, source: readonly Country[]): Country[] {
	try {
		const names = new Intl.DisplayNames([locale], { type: 'region', fallback: 'none' });
		const order = new Intl.Collator(locale);
		return source
			.map((country) => ({ code: country.code, name: names.of(country.code) ?? country.name }))
			.sort((a, b) => order.compare(a.name, b.name) || a.code.localeCompare(b.code, 'en'));
	} catch {
		return source
			.map((country) => ({ ...country }))
			.sort((a, b) => (a.code < b.code ? -1 : a.code > b.code ? 1 : 0));
	}
}
export function dateTime(locale: Locale): (value: number) => string {
	try {
		const formatter = new Intl.DateTimeFormat(locale, {
			year: 'numeric',
			month: 'short',
			day: '2-digit',
			hour: '2-digit',
			minute: '2-digit',
			second: '2-digit',
			hourCycle: 'h23',
			timeZone: 'UTC',
			timeZoneName: 'short'
		});
		return (value) => formatter.format(value);
	} catch {
		return (value) => `${new Date(value).toISOString().replace('T', ' ').slice(0, 19)} UTC`;
	}
}
