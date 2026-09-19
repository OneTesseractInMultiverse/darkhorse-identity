<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import LoginPanel from './LoginPanel.svelte';
	import type { AuthState } from '$lib/authentication';
	import type { EmailStatus, EmailResult } from '$lib/email-verification';
	let {
		read,
		request,
		confirm,
		takeToken,
		watchToken,
		signIn,
		checkSession,
		signOut
	}: {
		read: () => Promise<EmailStatus>;
		request: () => Promise<EmailResult>;
		confirm: (token: string) => Promise<EmailResult>;
		takeToken: () => string | undefined;
		watchToken?: (changed: () => void) => () => void;
		signIn: (email: string, password: string) => Promise<AuthState>;
		checkSession: () => Promise<AuthState>;
		signOut: () => Promise<boolean>;
	} = $props();
	let account = $state<EmailStatus>({ kind: 'unavailable' });
	let pending = $state(true);
	let token = $state<string | undefined>();
	let message = $state('');
	let uncertain = $state(false);
	onMount(() => {
		let mounted = true;
		// Initial hydration must finish before the router can replace history.
		void tick().then(() => {
			if (mounted) receive();
		});
		const stop = watchToken?.(receive);
		return () => {
			mounted = false;
			stop?.();
		};
	});
	function receive() {
		token = takeToken();
		void refresh();
	}
	async function refresh() {
		pending = true;
		account = await read();
		if (account.kind === 'ready') {
			uncertain = false;
			if (account.verified) token = undefined;
		}
		pending = false;
	}
	async function authenticate(email: string, password: string) {
		const result = await signIn(email, password);
		if (result.kind === 'signed-in') await refresh();
		return result;
	}
	async function changeAccount() {
		pending = true;
		if (await signOut()) await refresh();
		else message = 'Sign out could not be confirmed. Please try again.';
		pending = false;
	}
	async function submit() {
		if (pending || uncertain) return;
		pending = true;
		const confirming = token !== undefined;
		const result = confirming ? await confirm(token!) : await request();
		if (result === 'ok') {
			message = confirming
				? 'Your email has been verified.'
				: 'A verification message has been queued. Check your inbox. The link expires 15 minutes after your request.';
			if (confirming) token = undefined;
			await refresh();
		} else if (result === 'signed-out') {
			account = { kind: 'signed-out' };
			message = 'Sign in to the account that requested this link.';
		} else if (result === 'unavailable') {
			uncertain = true;
			message = 'The result could not be confirmed. Refresh the status before trying again.';
		} else if (result === 'limited')
			message =
				'Please wait before requesting another message. Requests are limited to one per 15 minutes and five per day.';
		else
			message =
				'This link has expired, was used, or belongs to a different account. Sign in to the account that requested it, or open the email settings again to request a new link.';
		pending = false;
	}
</script>

<section class="glass email-panel" aria-labelledby="email-title" aria-busy={pending}>
	<div class="panel-top">
		<span><span class="dot"></span> ACCOUNT SECURITY</span><span>EMAIL</span>
	</div>
	<div class="panel-body">
		<h1 id="email-title">Verify your email.</h1>
		<p class="intro">Confirm that you can receive messages at your account’s email address.</p>
		{#if account.kind === 'ready'}
			<p class="address">{account.email}</p>
			<p class="verification-status">
				{account.verified ? 'Email verified' : 'Email not verified'}
			</p>
			{#if !account.verified}
				<Button onclick={submit} disabled={pending || uncertain} class="mt-6 h-11 w-full font-mono"
					>{token ? 'Confirm email' : 'Send verification email'}</Button
				>
				{#if token}<p class="hint">
						Confirm only if you requested this link for the account shown above.
					</p>{/if}
			{/if}
			<Button variant="outline" onclick={changeAccount} disabled={pending} class="mt-3 w-full"
				>Sign in with another account</Button
			>
		{:else if account.kind === 'disabled'}<p>
				Email verification is not enabled for this deployment.
			</p>
		{:else if account.kind === 'unavailable' && !pending}<p role="alert">
				Email verification is temporarily unavailable.
			</p>
		{/if}
		{#if message}<p role="status" class="notice">{message}</p>{/if}
		<Button variant="outline" onclick={refresh} disabled={pending} class="mt-3 w-full"
			>Refresh status</Button
		>
	</div>
</section>
{#if account.kind === 'signed-out'}<LoginPanel signIn={authenticate} {checkSession} />{/if}

<style>
	.email-panel {
		width: min(100%, 34rem);
		margin: 0 auto;
	}
	.address {
		overflow-wrap: anywhere;
		font-family: var(--font-mono);
		color: var(--color-primary);
	}
	.verification-status {
		margin-top: 1rem;
	}
	.notice,
	.hint {
		margin-top: 1rem;
		line-height: 1.6;
	}
</style>
