<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	import { Button } from '$lib/components/ui/button';
	import AccountOverview from './AccountOverview.svelte';
	import type { AuthState } from '$lib/authentication';
	let {
		signIn,
		checkSession,
		signOut,
		showSecurityLink
	}: {
		signIn: (email: string, password: string) => Promise<AuthState>;
		checkSession: () => Promise<AuthState>;
		signOut?: () => Promise<boolean>;
		showSecurityLink?: boolean;
	} = $props();
	let email = $state('');
	let password = $state('');
	let pending = $state(true);
	let account = $state<AuthState>({ kind: 'signed-out' });
	let errorKey = $state<
		'login.invalid' | 'login.limited' | 'login.unavailable' | 'logout.unconfirmed' | undefined
	>();
	const error = $derived(errorKey ? $language.t(errorKey) : '');
	let message: HTMLParagraphElement | undefined = $state();
	const errors = {
		'signed-out': 'login.invalid',
		limited: 'login.limited',
		unavailable: 'login.unavailable'
	} as const;
	onMount(() => {
		void restore();
	});
	async function restore() {
		account = await checkSession();
		if (account.kind === 'unavailable') errorKey = errors.unavailable;
		pending = false;
	}
	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (pending) return;
		pending = true;
		errorKey = undefined;
		const supplied = password;
		password = '';
		account = await signIn(email, supplied);
		pending = false;
		if (account.kind !== 'signed-in') {
			errorKey = errors[account.kind];
			await tick();
			message?.focus();
		}
	}
	async function logout() {
		pending = true;
		errorKey = undefined;
		if (await signOut?.()) account = { kind: 'signed-out' };
		else errorKey = 'logout.unconfirmed';
		pending = false;
	}
</script>

{#if account.kind === 'signed-in' && showSecurityLink}
	<AccountOverview name={account.name} {pending} {error} signOut={signOut ? logout : undefined} />
{:else}
	<section
		class="glass login-panel"
		lang={$language.locale}
		aria-labelledby="login-title"
		aria-busy={pending}
	>
		<div class="panel-top">
			<span><span class="dot"></span> {$language.t('login.secure')}</span><span
				>{$language.t('login.identity')}</span
			>
		</div>
		<div class="panel-body">
			<span class="eyebrow">{$language.t('login.tagline')}</span>
			{#if account.kind === 'signed-in'}
				<h1 id="login-title">{$language.t('login.welcome', { name: account.name })}</h1>
				<p class="intro">{$language.t('login.signedIn')}</p>
				{#if signOut}<Button onclick={logout} disabled={pending} class="mt-6 h-11 w-full font-mono"
						>{$language.t(pending ? 'logout.pending' : 'logout.submit')}</Button
					>{/if}
			{:else}
				<h1 id="login-title">{$language.t('login.heading')}</h1>
				<p class="intro">{$language.t('login.intro')}</p>
				<form aria-label={$language.t('login.submit')} onsubmit={submit} class="login-form">
					<label for="email">{$language.t('login.email')}</label>
					<input
						id="email"
						name="email"
						type="email"
						autocomplete="username"
						autocapitalize="none"
						spellcheck="false"
						required
						maxlength="254"
						bind:value={email}
						disabled={pending}
					/>
					<label for="password">{$language.t('login.password')}</label>
					<input
						id="password"
						name="password"
						type="password"
						autocomplete="current-password"
						required
						maxlength="512"
						bind:value={password}
						disabled={pending}
						aria-describedby={error ? 'login-error' : undefined}
					/>
					<Button type="submit" disabled={pending} class="mt-3 h-12 w-full font-mono"
						>{$language.t(pending ? 'login.pending' : 'login.submit')}
						<span aria-hidden="true">↗</span></Button
					>
				</form>
			{/if}
			{#if error}<p
					id="login-error"
					bind:this={message}
					role="alert"
					tabindex="-1"
					class="login-error"
				>
					{error}
				</p>{/if}
		</div>
	</section>
{/if}
