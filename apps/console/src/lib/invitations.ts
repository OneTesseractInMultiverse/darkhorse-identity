export type InvitationInput = {
	token: string;
	email: string;
	first_name: string;
	last_name: string;
	password: string;
};
export type InvitationResult = 'ok' | 'invalid' | 'limited' | 'unavailable';
export function invitationToken(fragment: string): string | undefined {
	return fragment.length === 75 && /^#token=iv1_[0-9a-f]{64}$/.test(fragment)
		? fragment.slice(7)
		: undefined;
}
export async function acceptInvitation(
	fetcher: typeof fetch,
	input: InvitationInput
): Promise<InvitationResult> {
	try {
		const response = await fetcher('/api/invitations/accept', {
			method: 'POST',
			credentials: 'same-origin',
			cache: 'no-store',
			headers: { 'content-type': 'application/json', 'x-darkhorse-csrf': '1' },
			body: JSON.stringify(input)
		});
		if (response.status === 400) return 'invalid';
		if (response.status === 429) return 'limited';
		if (response.status !== 200) return 'unavailable';
		const value: unknown = await response.json();
		if (typeof value === 'object' && value !== null && 'ok' in value && value.ok === true)
			return 'ok';
	} catch {
		/* The account may have been created. Never resubmit automatically. */
	}
	return 'unavailable';
}
