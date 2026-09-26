<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
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
	let message = $state<
		| 'logout.unconfirmed'
		| 'email.confirmed'
		| 'email.queued'
		| 'email.wrongAccount'
		| 'email.uncertain'
		| 'email.limited'
		| 'email.invalid'
		| null
	>(null);
	let uncertain = $state(false);
	let mounted = false;
	onMount(() => {
		mounted = true;
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
		const result = await read();
		if (!mounted) return;
		account = result;
		if (account.kind === 'ready') {
			uncertain = false;
			if (account.verified) token = undefined;
		}
		pending = false;
	}
	async function authenticate(email: string, password: string) {
		const result = await signIn(email, password);
		if (!mounted) return result;
		if (result.kind === 'signed-in') {
			language.account(result.locale);
			await refresh();
		}
		return result;
	}
	async function changeAccount() {
		pending = true;
		const confirmed = await signOut();
		if (!mounted) return;
		if (confirmed) {
			language.account(undefined);
			await refresh();
		} else message = 'logout.unconfirmed';
		pending = false;
	}
	async function submit() {
		if (pending || uncertain) return;
		pending = true;
		const confirming = token !== undefined;
		const result = confirming ? await confirm(token!) : await request();
		if (!mounted) return;
		if (result === 'ok') {
			message = confirming ? 'email.confirmed' : 'email.queued';
			if (confirming) token = undefined;
			await refresh();
		} else if (result === 'signed-out') {
			account = { kind: 'signed-out' };
			language.account(undefined);
			message = 'email.wrongAccount';
		} else if (result === 'unavailable') {
			uncertain = true;
			message = 'email.uncertain';
		} else if (result === 'limited') message = 'email.limited';
		else message = 'email.invalid';
		pending = false;
	}
</script>

<section class="glass email-panel" aria-labelledby="email-title" aria-busy={pending}>
	<div class="panel-top">
		<span><span class="dot"></span> {$language.t('sessions.security')}</span><span
			>{$language.t('email.edition')}</span
		>
	</div>
	<div class="panel-body">
		<h1 id="email-title">{$language.t('email.heading')}</h1>
		<p class="intro">{$language.t('email.intro')}</p>
		{#if account.kind === 'ready'}
			<p class="address">{account.email}</p>
			<p class="verification-status">
				{$language.t(account.verified ? 'email.verified' : 'email.unverified')}
			</p>
			{#if !account.verified}
				<Button onclick={submit} disabled={pending || uncertain} class="mt-6 h-11 w-full font-mono"
					>{$language.t(token ? 'email.confirm' : 'email.send')}</Button
				>
				{#if token}<p class="hint">
						{$language.t('email.confirmHelp')}
					</p>{/if}
			{/if}
			<Button variant="outline" onclick={changeAccount} disabled={pending} class="mt-3 w-full"
				>{$language.t('email.changeAccount')}</Button
			>
		{:else if account.kind === 'disabled'}<p>
				{$language.t('email.disabled')}
			</p>
		{:else if account.kind === 'unavailable' && !pending}<p role="alert">
				{$language.t('email.unavailable')}
			</p>
		{/if}
		{#if message}<p role="status" class="notice">{$language.t(message)}</p>{/if}
		<Button variant="outline" onclick={refresh} disabled={pending} class="mt-3 w-full"
			>{$language.t('email.refresh')}</Button
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
