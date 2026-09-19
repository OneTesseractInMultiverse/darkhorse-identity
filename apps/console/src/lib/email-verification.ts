export type EmailStatus =
	| { kind: 'ready'; email: string; verified: boolean }
	| { kind: 'signed-out' | 'unavailable' | 'disabled' };
export type EmailResult = 'ok' | 'invalid' | 'signed-out' | 'limited' | 'unavailable';
export function fragmentToken(fragment: string): string | undefined {
	return fragment.length === 75 && /^#token=ev1_[0-9a-f]{64}$/.test(fragment)
		? fragment.slice(7)
		: undefined;
}
export async function emailStatus(fetcher: typeof fetch): Promise<EmailStatus> {
	try {
		const response = await fetcher('/api/security/email', {
			credentials: 'same-origin',
			cache: 'no-store'
		});
		if (response.status === 401) return { kind: 'signed-out' };
		if (response.status === 404) return { kind: 'disabled' };
		if (!response.ok) return { kind: 'unavailable' };
		const value: unknown = await response.json();
		if (
			typeof value === 'object' &&
			value !== null &&
			'email' in value &&
			typeof value.email === 'string' &&
			value.email.length > 0 &&
			value.email.length <= 254 &&
			'verified' in value &&
			typeof value.verified === 'boolean'
		)
			return { kind: 'ready', email: value.email, verified: value.verified };
	} catch {
		/* A lost or malformed response is never evidence of account state. */
	}
	return { kind: 'unavailable' };
}
export async function requestEmail(fetcher: typeof fetch): Promise<EmailResult> {
	return mutate(fetcher, '/api/security/email/request', {});
}
export async function confirmEmail(fetcher: typeof fetch, token: string): Promise<EmailResult> {
	return mutate(fetcher, '/api/security/email/confirm', { token });
}
async function mutate(fetcher: typeof fetch, path: string, body: object): Promise<EmailResult> {
	try {
		const response = await fetcher(path, {
			method: 'POST',
			credentials: 'same-origin',
			cache: 'no-store',
			headers: { 'content-type': 'application/json', 'x-darkhorse-csrf': '1' },
			body: JSON.stringify(body)
		});
		if (response.status === 401) return 'signed-out';
		if (response.status === 400) return 'invalid';
		if (response.status === 429) return 'limited';
		if (response.status !== 200) return 'unavailable';
		const value: unknown = await response.json();
		if (typeof value === 'object' && value !== null && 'ok' in value && value.ok === true)
			return 'ok';
	} catch {
		/* The mutation may have committed; the caller reconciles by reading. */
	}
	return 'unavailable';
}
