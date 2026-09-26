import type { Locale } from './locale';
/** Fixed fragment grammar; language is a page hint and the proof is never a URL query. */
export function emailLink(
	fragment: string,
	purpose: 'ev1' | 'iv1'
): { token: string; locale?: Locale } | undefined {
	if (fragment.length !== 75 && fragment.length !== 83) return undefined;
	const match = /^#token=((?:ev1|iv1)_[0-9a-f]{64})(?:&lang=(en|es))?$/.exec(fragment);
	if (!match || match[0] !== fragment || !match[1].startsWith(`${purpose}_`)) return undefined;
	return { token: match[1], locale: match[2] as Locale | undefined };
}
