<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
	import {
		formatTime,
		type SessionRecord,
		type SessionsState,
		type Termination
	} from '$lib/sessions';
	let {
		read,
		end
	}: {
		read: (after?: string) => Promise<SessionsState>;
		end: (id: string) => Promise<Termination>;
	} = $props();
	let sessionState = $state<SessionsState>({ kind: 'unavailable' });
	let pending = $state(true);
	let uncertain = $state(false);
	let older = $state(false);
	let selected = $state<SessionRecord | null>(null);
	let dialog = $state<HTMLDialogElement>();
	let message = $state('');
	let alert = $state<HTMLParagraphElement>();
	onMount(() => {
		void load();
	});
	$effect(() => {
		if (selected && dialog && !dialog.open) dialog.showModal();
	});
	async function load(after?: string) {
		pending = true;
		message = '';
		sessionState = await read(after);
		older = after !== undefined;
		if (sessionState.kind === 'ready') uncertain = false;
		else if (sessionState.kind === 'unavailable')
			message = 'Sessions are temporarily unavailable. Refresh the list to try again.';
		pending = false;
	}
	function cancel() {
		dialog?.close();
		selected = null;
	}
	async function confirm() {
		if (!selected || pending) return;
		pending = true;
		const result = await end(selected.id);
		cancel();
		if (result.kind === 'signed-out' || (result.kind === 'ended' && result.current)) {
			sessionState = { kind: 'signed-out' };
			pending = false;
			return;
		}
		if (result.kind === 'ended') {
			await load();
			return;
		}
		uncertain = true;
		pending = false;
		message = 'The change could not be confirmed. Refresh the session list before trying again.';
		await tick();
		alert?.focus();
	}
</script>

<section class="glass session-panel" aria-labelledby="sessions-title" aria-busy={pending}>
	<div class="panel-top">
		<span><span class="dot"></span> ACCOUNT SECURITY</span><span>SESSIONS / 01</span>
	</div>
	<div class="session-body">
		<div class="session-heading">
			<div>
				<h1 id="sessions-title">Your sessions.</h1>
				<p>Review your Darkhorse sign-ins and end a session you no longer use.</p>
			</div>
			{#if sessionState.kind !== 'signed-out'}<Button
					variant="outline"
					onclick={() => load()}
					disabled={pending}>Refresh sessions</Button
				>{/if}
		</div>
		{#if pending && sessionState.kind !== 'ready'}<p role="status">Loading sessions…</p>
		{:else if sessionState.kind === 'signed-out'}
			<p>Your session has ended. Sign in to manage your sessions.</p>
			<a class="security-link" href={resolve('/')}>Sign in</a>
		{:else if sessionState.kind === 'ready'}
			<div class="table-wrap">
				<table>
					<caption>Sign-in history · times shown in UTC</caption>
					<thead
						><tr
							><th scope="col">Started</th><th scope="col">Last activity</th><th scope="col"
								>Status</th
							><th scope="col">Actions</th></tr
						></thead
					>
					<tbody
						>{#each sessionState.page.items as session (session.id)}<tr>
								<th scope="row"
									><time datetime={new Date(session.created_ms).toISOString()}
										>{formatTime(session.created_ms)}</time
									>{#if session.id === sessionState.page.current}<span class="current"
											>This session</span
										>{/if}</th
								>
								<td
									><time datetime={new Date(session.seen_ms).toISOString()}
										>{formatTime(session.seen_ms)}</time
									></td
								>
								<td
									><span class:active={session.status === 'active'} class="state"
										>{session.status === 'active' ? 'Active' : 'Ended or expired'}</span
									></td
								>
								<td
									>{#if session.status === 'active'}<Button
											variant="outline"
											disabled={pending || uncertain}
											onclick={() => {
												selected = session;
											}}
											>{session.id === sessionState.page.current
												? 'End this session'
												: 'End session'}</Button
										>{:else}<span aria-label="No action needed">—</span>{/if}</td
								>
							</tr>{/each}</tbody
					>
				</table>
			</div>
			{#if sessionState.page.items.length === 0}<p class="empty">No sessions on this page.</p>{/if}
			<nav class="pagination" aria-label="Session pages">
				{#if older}<Button variant="outline" disabled={pending} onclick={() => load()}
						>Newest sessions</Button
					>{/if}
				{#if sessionState.page.next}<Button
						variant="outline"
						disabled={pending}
						onclick={() => {
							if (sessionState.kind === 'ready' && sessionState.page.next)
								void load(sessionState.page.next);
						}}>Older sessions</Button
					>{/if}
			</nav>
			<p class="session-help">
				Ending a session immediately stops its access to Darkhorse and connected APIs. Applications
				may still show their own signed-in page until their next access check.
			</p>
		{/if}
		{#if message}<p role="alert" tabindex="-1" bind:this={alert} class="login-error">
				{message}
			</p>{/if}
	</div>
</section>
{#if selected}
	<dialog
		bind:this={dialog}
		onclose={() => {
			selected = null;
		}}
		oncancel={(event) => {
			if (pending) event.preventDefault();
		}}
		aria-labelledby="end-session-title"
		aria-describedby="end-session-detail"
	>
		<h2 id="end-session-title">End this session?</h2>
		<p id="end-session-detail">
			The sign-in started at {formatTime(selected.created_ms)}. Its connected access will stop.
		</p>
		{#if sessionState.kind === 'ready' && selected.id === sessionState.page.current}<p>
				You will also be signed out of this page.
			</p>{/if}
		<div class="dialog-actions">
			<Button variant="outline" onclick={cancel} disabled={pending}>Cancel</Button><Button
				onclick={confirm}
				disabled={pending}>{pending ? 'Ending session…' : 'Confirm end session'}</Button
			>
		</div>
	</dialog>
{/if}

<style>
	.session-panel {
		min-width: 0;
		width: min(100%, 1120px);
	}
	.session-body {
		padding: 32px;
	}
	.session-heading {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 24px;
		margin-bottom: 24px;
	}
	h1 {
		font-size: 36px;
		margin: 0 0 14px;
		letter-spacing: -0.035em;
	}
	p {
		color: #b1c0b7;
		line-height: 1.65;
	}
	.table-wrap {
		overflow-x: auto;
		border: 1px solid #96b8a122;
		border-radius: 12px;
	}
	table {
		width: 100%;
		min-width: 740px;
		border-collapse: collapse;
		text-align: left;
	}
	caption {
		text-align: left;
		padding: 16px 20px;
		font:
			11px ui-monospace,
			monospace;
		color: #9aaea2;
	}
	th,
	td {
		padding: 18px 20px;
		border-top: 1px solid #96b8a122;
		font-size: 13px;
	}
	thead th {
		text-transform: uppercase;
		font:
			10px ui-monospace,
			monospace;
		letter-spacing: 0.1em;
		color: #9aaea2;
		background: #07110a50;
	}
	tbody th {
		font-weight: 400;
	}
	time {
		white-space: nowrap;
		font-family: ui-monospace, monospace;
		font-size: 12px;
	}
	.current {
		display: block;
		margin-top: 8px;
		color: #91e4b4;
		font-size: 11px;
	}
	.state {
		color: #9aaea2;
		white-space: nowrap;
		font-size: 12px;
	}
	.state.active {
		color: #91e4b4;
	}
	.pagination {
		display: flex;
		justify-content: flex-end;
		gap: 12px;
		margin-top: 20px;
	}
	.session-help {
		font-size: 12px;
		max-width: 780px;
		margin-top: 24px;
	}
	.empty {
		padding: 20px;
	}
	.security-link {
		display: inline-block;
		color: #91e4b4;
		margin-top: 12px;
	}
	dialog {
		max-width: min(520px, calc(100vw - 40px));
		margin: auto;
		padding: 30px;
		border-radius: 20px;
		border: 1px solid #96b8a150;
		background: #152019;
		color: #eaf0ed;
		box-shadow: 0 24px 80px #0009;
	}
	dialog::backdrop {
		background: #0009;
		backdrop-filter: blur(6px);
	}
	dialog h2 {
		font-size: 24px;
		margin: 0 0 16px;
	}
	.dialog-actions {
		display: flex;
		justify-content: flex-end;
		gap: 12px;
		margin-top: 28px;
	}
	@media (max-width: 600px) {
		.session-body {
			padding: 20px;
		}
		.session-heading {
			align-items: flex-start;
			flex-direction: column;
		}
		h1 {
			font-size: 30px;
		}
		.dialog-actions {
			flex-wrap: wrap;
		}
	}
</style>
