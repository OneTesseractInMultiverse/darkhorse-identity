<script lang="ts">
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
	import Modal from '$lib/components/admin/Modal.svelte';
	import KeyEditor from './KeyEditor.svelte';
	import {
		failureMessage,
		type KeyApi,
		type Page,
		type Key,
		type Creation,
		type Failure
	} from '$lib/personal-keys';
	import { formatTime } from '$lib/sessions';
	let { api }: { api: KeyApi } = $props();
	let page = $state<Page | null>(null);
	let pending = $state(true);
	let blocked = $state(false);
	let signedOut = $state(false);
	let reauthenticate = $state(false);
	let message = $state('');
	let modal = $state<'create' | 'detail' | 'revoke' | 'secret' | null>(null);
	let selected = $state<Key | null>(null);
	let secret = $state('');
	let mounted = false;
	onMount(() => {
		mounted = true;
		void load();
		return () => {
			mounted = false;
			secret = '';
		};
	});
	function concealed() {
		if (secret) {
			secret = '';
			modal = null;
			message =
				'The secret is no longer displayed. If it was not saved, revoke the key and create a replacement.';
		}
	}
	function close() {
		secret = '';
		selected = null;
		modal = null;
	}
	function failure(result: Failure) {
		message = failureMessage(result);
		blocked = true;
		reauthenticate = result.kind === 'reauthenticate';
		if (result.kind === 'signed-out') {
			signedOut = true;
			page = null;
		}
		close();
	}
	async function load(after?: string) {
		pending = true;
		message = '';
		const result = await api.list(after);
		if (!mounted) return;
		if (result.kind === 'ready') {
			page = result.value;
			blocked = false;
			signedOut = false;
			reauthenticate = false;
		} else {
			page = null;
			failure(result);
		}
		pending = false;
	}
	async function create(input: Creation) {
		pending = true;
		message = '';
		const result = await api.create(input);
		if (!mounted) return;
		if (result.kind === 'created') {
			selected = result.key;
			page = null;
			if (document.visibilityState === 'hidden') {
				close();
				message =
					'The key was created while this page was hidden. Its secret cannot be retrieved. Refresh the list, revoke that key, and create a replacement.';
				blocked = true;
			} else {
				secret = result.secret;
				modal = 'secret';
			}
		} else failure(result);
		pending = false;
	}
	async function revoke() {
		if (!selected || pending) return;
		pending = true;
		message = '';
		const result = await api.revoke(selected.id);
		if (!mounted) return;
		close();
		if (result.kind === 'revoked') {
			await load();
			message = 'API key revoked.';
		} else failure(result);
		pending = false;
	}
	function show(key: Key, kind: 'detail' | 'revoke') {
		selected = key;
		modal = kind;
		message = '';
	}
	async function acknowledge() {
		close();
		await load();
	}
</script>

<svelte:window onpagehide={concealed} /><svelte:document
	onvisibilitychange={() => {
		if (document.visibilityState === 'hidden') concealed();
	}}
/>
<section class="glass key-panel" aria-labelledby="keys-title" aria-busy={pending}>
	<div class="directory-heading">
		<div>
			<p class="eyebrow">ACCOUNT SECURITY</p>
			<h1 id="keys-title">Personal API keys.</h1>
			<p>Give scripts and services a limited portion of your application access.</p>
		</div>
		{#if !signedOut}<div class="directory-actions">
				<Button variant="outline" disabled={pending} onclick={() => load()}>Refresh keys</Button
				><Button
					disabled={pending || blocked || !page}
					onclick={() => {
						modal = 'create';
						message = '';
					}}>New API key</Button
				>
			</div>{/if}
	</div>
	{#if message}<p role="alert" class="directory-notice">{message}</p>{/if}
	{#if signedOut || reauthenticate}<a class="admin-link" href={resolve('/')}>Sign in</a>{/if}
	{#if pending && !page}<p role="status">Loading…</p>{:else if page}
		<p class="key-scroll-hint">Scroll sideways to view all columns. The key name stays visible.</p>
		<!-- svelte-ignore a11y_no_noninteractive_tabindex (keyboard users must be able to scroll the table horizontally) -->
		<div class="directory-table" role="region" aria-label="Your API keys" tabindex="0">
			<table>
				<thead
					><tr><th>Name</th><th>Created</th><th>Expires</th><th>Status</th><th>Actions</th></tr
					></thead
				><tbody>
					{#each page.items as key (key.id)}<tr
							><td>{key.name}</td><td>{formatTime(key.created_ms)}</td><td
								>{key.expires_ms === null ? 'No expiration' : formatTime(key.expires_ms)}</td
							><td>{key.active ? 'Active' : 'Inactive'}</td><td
								><div class="directory-actions">
									<Button
										variant="outline"
										disabled={pending}
										onclick={() => show(key, 'detail')}
										aria-label={`View ${key.name}`}>View</Button
									><Button
										variant="outline"
										disabled={pending || blocked || !key.active}
										onclick={() => show(key, 'revoke')}
										aria-label={`Revoke ${key.name}`}>Revoke</Button
									>
								</div></td
							></tr
						>{/each}
					{#if page.items.length === 0}<tr><td colspan="5">You have no API keys yet.</td></tr>{/if}
				</tbody>
			</table>
		</div>
		{#if page.next}<Button variant="outline" disabled={pending} onclick={() => load(page!.next!)}
				>Next keys</Button
			>{/if}
	{/if}
	<p class="key-hint">
		Keys do not grow when you receive new permissions. Signing out ends your browser session; revoke
		API keys separately when they are no longer needed.
	</p>
</section>
{#if modal === 'create'}<Modal title="New personal API key" {pending} {close}
		><KeyEditor read={api.options} {create} cancel={close} {pending} /></Modal
	>
{:else if modal === 'secret' && selected}<Modal
		title="Save your API key"
		{pending}
		close={() => {
			void acknowledge();
		}}
		><p>Store this secret securely now. Darkhorse cannot display it again.</p>
		<label for="key-secret">API key secret</label><textarea
			id="key-secret"
			class="key-secret"
			readonly
			value={secret}
			spellcheck="false"
			autocomplete="off"
			rows="3"></textarea>
		<p>
			{selected.name} · {selected.expires_ms === null
				? 'No expiration'
				: `Expires ${formatTime(selected.expires_ms)}`}
		</p>
		<Button onclick={acknowledge}>I have saved the key</Button></Modal
	>
{:else if modal === 'revoke' && selected}<Modal title="Revoke API key" {pending} {close}
		><p>
			Revoke <strong>{selected.name}</strong>? This cannot be undone. Services using this key will
			lose access.
		</p>
		<div class="directory-actions">
			<Button disabled={pending} onclick={revoke}>Confirm revoke key</Button><Button
				variant="outline"
				disabled={pending}
				onclick={close}>Cancel</Button
			>
		</div></Modal
	>
{:else if modal === 'detail' && selected}<Modal title={selected.name} {close}
		><p class="key-hint">
			These are the original permission limits. Current account access may reduce them.
		</p>
		<p>Application: <code>{selected.application_id}</code></p>
		{#each selected.grants as grant (grant.resource_id)}<h3>
				Resource <code>{grant.resource_id}</code>
			</h3>
			<ul>
				{#each grant.capabilities as cap (cap)}<li><code>{cap}</code></li>{/each}
			</ul>{/each}<Button onclick={close}>Close</Button></Modal
	>{/if}
