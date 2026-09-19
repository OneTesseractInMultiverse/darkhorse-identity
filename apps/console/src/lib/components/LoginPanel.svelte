<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
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
	let error = $state('');
	let message: HTMLParagraphElement | undefined = $state();
	const errors = {
		'signed-out': 'Unable to sign in with those credentials.',
		limited: 'Too many attempts. Wait a moment before trying again.',
		unavailable: 'Sign in is temporarily unavailable. Please try again.'
	};
	onMount(() => {
		void restore();
	});
	async function restore() {
		account = await checkSession();
		if (account.kind === 'unavailable') error = errors.unavailable;
		pending = false;
	}
	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (pending) return;
		pending = true;
		error = '';
		const supplied = password;
		password = '';
		account = await signIn(email, supplied);
		pending = false;
		if (account.kind !== 'signed-in') {
			error = errors[account.kind];
			await tick();
			message?.focus();
		}
	}
	async function logout() {
		pending = true;
		error = '';
		if (await signOut?.()) account = { kind: 'signed-out' };
		else error = 'Sign out could not be confirmed. Please try again.';
		pending = false;
	}
</script>

<section class="glass login-panel" aria-labelledby="login-title" aria-busy={pending}>
	<div class="panel-top">
		<span><span class="dot"></span> SECURE ACCESS</span><span>IDENTITY / 01</span>
	</div>
	<div class="panel-body">
		<span class="eyebrow">YOUR ORGANIZATION. ONE IDENTITY.</span>
		{#if account.kind === 'signed-in'}
			<h1 id="login-title">Welcome, {account.name}.</h1>
			<p class="intro">You're signed in to Darkhorse.</p>
			{#if showSecurityLink}<a
					href={resolve('/security/sessions')}
					class="text-primary underline underline-offset-4">Manage sessions</a
				>{/if}
			{#if signOut}<Button onclick={logout} disabled={pending} class="mt-6 h-11 w-full font-mono"
					>{pending ? 'Signing out…' : 'Sign out'}</Button
				>{/if}
		{:else}
			<h1 id="login-title">Welcome<br /><span>back.</span></h1>
			<p class="intro">Sign in to your organization’s workspace.</p>
			<form aria-label="Sign in" onsubmit={submit} class="login-form">
				<label for="email">Email address</label>
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
				<label for="password">Password</label>
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
					>{pending ? 'Connecting…' : 'Sign in'} <span aria-hidden="true">↗</span></Button
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
