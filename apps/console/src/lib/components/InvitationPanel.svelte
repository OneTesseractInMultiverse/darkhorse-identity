<script lang="ts">
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
	let error = $state('');
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
		error = '';
	}
	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (pending || !token) return;
		if (password !== confirmation) {
			error = 'The passwords must match.';
			return;
		}
		pending = true;
		error = '';
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
		<span><span class="dot"></span> SECURE ENROLLMENT</span><span>IDENTITY / NEW</span>
	</div>
	<div class="panel-body">
		<span class="eyebrow">YOUR ORGANIZATION. ONE IDENTITY.</span>
		<h1 id="invitation-title">Create your<br /><span>account.</span></h1>
		{#if result === 'ok'}
			<p role="status" tabindex="-1" bind:this={message}>
				Your account is ready. Sign in with your new password. Your administrator assigns
				application access separately.
			</p>
		{:else if result}
			<p role="alert" tabindex="-1" bind:this={message}>
				{#if result === 'invalid'}This invitation could not be accepted. Check the email and account
					details, or ask your administrator for a new invitation.
				{:else if result === 'limited'}The invitation attempt limit has been reached. Wait before
					reopening the email link, or ask your administrator for a new invitation.
				{:else}We could not confirm whether your account was created. Try signing in first. If that
					does not work, reopen your invitation later.{/if}
			</p>
		{:else if token}
			<p class="intro">
				Use the email address that received your invitation. Choose a password with 15–128
				characters.
			</p>
			<form aria-label="Accept invitation" onsubmit={submit} class="login-form">
				<label for="invite-email">Email address</label><input
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
				<label for="invite-first">First name</label><input
					id="invite-first"
					autocomplete="given-name"
					required
					maxlength="100"
					bind:value={firstName}
					disabled={pending}
				/>
				<label for="invite-last">Last name</label><input
					id="invite-last"
					autocomplete="family-name"
					required
					maxlength="100"
					bind:value={lastName}
					disabled={pending}
				/>
				<label for="invite-password">Password</label><input
					id="invite-password"
					type="password"
					autocomplete="new-password"
					required
					minlength="15"
					maxlength="256"
					bind:value={password}
					disabled={pending}
				/>
				<label for="invite-confirmation">Confirm password</label><input
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
					>{pending ? 'Creating account…' : 'Create account'}</Button
				>
			</form>
		{:else}<p class="intro">
				Open the invitation link sent to your email to create an account.
			</p>{/if}
		{#if error}<p role="alert">{error}</p>{/if}
		<a href={resolve('/')} class="mt-6 inline-block text-primary underline underline-offset-4"
			>Go to sign in</a
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
