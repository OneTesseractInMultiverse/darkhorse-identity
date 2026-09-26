<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	import { onMount, tick } from 'svelte';
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
	import type { InvitationInput, InvitationResult } from '$lib/invitations';
	let {
		accept,
		takeToken,
		watchToken
	}: {
		accept: (input: InvitationInput) => Promise<InvitationResult>;
		takeToken: () => string | undefined;
		watchToken: (changed: () => void) => () => void;
	} = $props();
	let token = $state<string>();
	let email = $state('');
	let firstName = $state('');
	let lastName = $state('');
	let password = $state('');
	let confirmation = $state('');
	let pending = $state(false);
	let result = $state<InvitationResult>();
	let mismatch = $state(false);
	let message: HTMLParagraphElement | undefined = $state();
	onMount(() => {
		let mounted = true;
		// Initial hydration must finish before the router can replace history.
		void tick().then(() => {
			if (mounted) receive();
		});
		const stop = watchToken(receive);
		return () => {
			mounted = false;
			stop();
		};
	});
	function receive() {
		const supplied = takeToken();
		if (pending) return;
		token = supplied;
		password = '';
		confirmation = '';
		result = undefined;
		mismatch = false;
	}
	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (pending || !token) return;
		if (password !== confirmation) {
			mismatch = true;
			return;
		}
		pending = true;
		mismatch = false;
		const input = { token, email, first_name: firstName, last_name: lastName, password };
		password = '';
		confirmation = '';
		result = await accept(input);
		pending = false;
		token = undefined;
		await tick();
		message?.focus();
	}
</script>

<section class="glass invitation-panel" aria-labelledby="invitation-title" aria-busy={pending}>
	<div class="panel-top">
		<span><span class="dot"></span> {$language.t('invitation.secure')}</span><span
			>{$language.t('invitation.new')}</span
		>
	</div>
	<div class="panel-body">
		<span class="eyebrow">{$language.t('invitation.tagline')}</span>
		<h1 id="invitation-title">{$language.t('invitation.heading')}</h1>
		{#if result === 'ok'}
			<p role="status" tabindex="-1" bind:this={message}>
				{$language.t('invitation.created')}
			</p>
		{:else if result}
			<p role="alert" tabindex="-1" bind:this={message}>
				{#if result === 'invalid'}{$language.t('invitation.invalid')}
				{:else if result === 'limited'}{$language.t('invitation.limited')}
				{:else}{$language.t('invitation.uncertain')}{/if}
			</p>
		{:else if token}
			<p class="intro">
				{$language.t('invitation.intro')}
			</p>
			<form aria-label={$language.t('invitation.accept')} onsubmit={submit} class="login-form">
				<label for="invite-email">{$language.t('login.email')}</label><input
					id="invite-email"
					type="email"
					autocomplete="username"
					autocapitalize="none"
					spellcheck="false"
					required
					maxlength="254"
					bind:value={email}
					disabled={pending}
				/>
				<label for="invite-first">{$language.t('profile.firstName')}</label><input
					id="invite-first"
					autocomplete="given-name"
					required
					maxlength="100"
					bind:value={firstName}
					disabled={pending}
				/>
				<label for="invite-last">{$language.t('profile.lastName')}</label><input
					id="invite-last"
					autocomplete="family-name"
					required
					maxlength="100"
					bind:value={lastName}
					disabled={pending}
				/>
				<label for="invite-password">{$language.t('login.password')}</label><input
					id="invite-password"
					type="password"
					autocomplete="new-password"
					required
					minlength="15"
					maxlength="256"
					bind:value={password}
					disabled={pending}
				/>
				<label for="invite-confirmation">{$language.t('invitation.confirmPassword')}</label><input
					id="invite-confirmation"
					type="password"
					autocomplete="new-password"
					required
					minlength="15"
					maxlength="256"
					bind:value={confirmation}
					disabled={pending}
				/>
				<Button type="submit" disabled={pending} class="mt-3 h-12 w-full font-mono"
					>{$language.t(pending ? 'invitation.creating' : 'invitation.create')}</Button
				>
			</form>
		{:else}<p class="intro">
				{$language.t('invitation.openLink')}
			</p>{/if}
		{#if mismatch}<p role="alert">{$language.t('invitation.mismatch')}</p>{/if}
		<a href={resolve('/')} class="mt-6 inline-block text-primary underline underline-offset-4"
			>{$language.t('invitation.goSignIn')}</a
		>
	</div>
</section>

<style>
	.invitation-panel {
		width: min(100%, 32rem);
		margin: auto;
	}
	p {
		line-height: 1.6;
	}
</style>
