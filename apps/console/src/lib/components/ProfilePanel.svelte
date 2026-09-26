<script lang="ts">
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import {
		message,
		type ProfileApi,
		type Profile,
		type Options,
		type Fields,
		type Failure
	} from '$lib/profiles';
	import ImageControl from './ImageControl.svelte';
	import avatar from '$lib/assets/brand/avatar.svg';
	import type { MediaApi } from '$lib/media';
	import Modal from './admin/Modal.svelte';
	import ProfileEditor from './ProfileEditor.svelte';
	import LanguageEditor from './LanguageEditor.svelte';
	import { useLocalization } from '$lib/i18n/context';
	import type { Locale } from '$lib/i18n/locale';
	const language = useLocalization();
	let languageEditing = $state(false);
	let {
		api,
		images,
		target = 'me'
	}: { api: ProfileApi; images?: MediaApi; target?: string } = $props();
	let pictureEditing = $state(false),
		pictureFailed = $state(false);
	let profile = $state<Profile | null>(null),
		options = $state<Options | null>(null),
		pending = $state(true),
		editing = $state(false),
		blocked = $state(false),
		error = $state(''),
		notice = $state('');
	let alive = true;
	function fail(e: Failure) {
		error = message(e);
		blocked = true;
		if (e.kind === 'signed-out' || e.kind === 'denied') {
			if (target === 'me') language.account(undefined);
			profile = null;
			options = null;
		}
	}
	async function load() {
		if (!alive) return;
		pending = true;
		editing = false;
		languageEditing = false;
		pictureEditing = false;
		pictureFailed = false;
		notice = '';
		error = '';
		const result = await api.load(target);
		if (!alive) return;
		if (result.kind === 'ready') {
			profile = result.value;
			if (target === 'me') language.account(profile.preferred_locale ?? undefined);
			blocked = false;
		} else fail(result);
		pending = false;
	}
	async function edit() {
		pending = true;
		notice = '';
		const result = await api.options();
		if (!alive) return;
		if (result.kind === 'ready') {
			options = result.value;
			editing = true;
		} else fail(result);
		pending = false;
	}
	async function save(fields: Fields) {
		if (!profile || pending || blocked) return;
		pending = true;
		error = '';
		const result = await api.save(target, profile.revision, fields);
		if (!alive) return;
		editing = false;
		if (result.kind === 'ready') {
			profile = result.value;
			if (target === 'me') language.account(profile.preferred_locale ?? undefined);
			notice = 'Profile saved.';
		} else fail(result);
		pending = false;
	}
	async function saveLanguage(locale: Locale | null) {
		if (!profile || pending || blocked || target !== 'me') return;
		pending = true;
		error = '';
		notice = '';
		const result = await api.language(profile.revision, locale);
		if (!alive) return;
		languageEditing = false;
		if (result.kind === 'ready') {
			profile = result.value;
			language.account(profile.preferred_locale ?? undefined);
			notice = 'Language preference saved.';
		} else fail(result);
		pending = false;
	}

	onMount(() => {
		alive = true;
		void load();
		return () => {
			alive = false;
			profile = null;
			options = null;
		};
	});
</script>

<section class="glass profile-panel" aria-labelledby="profile-title">
	<p class="eyebrow">DIRECTORY / PROFILE</p>
	<h1 id="profile-title">{target === 'me' ? 'My profile' : 'User profile'}</h1>
	<p>Contact details and the name shared with connected applications.</p>
	{#if error}<p role="alert">{error}</p>{/if}{#if notice}<p role="status">{notice}</p>{/if}
	{#if pending}<p role="status">Loading…</p>{/if}
	{#if profile}
		{#key profile.revision}<img
				class="avatar"
				src={pictureFailed ? avatar : `/api/profiles/${encodeURIComponent(target)}/picture`}
				alt="User avatar"
				width="96"
				height="96"
				onerror={() => {
					pictureFailed = true;
				}}
			/>{/key}
		<dl class="profile-details">
			<div>
				<dt>Email / username</dt>
				<dd>{profile.email} ({profile.email_verified ? 'verified' : 'unverified'})</dd>
			</div>
			<div>
				<dt>Status</dt>
				<dd>{profile.active ? 'Active' : 'Inactive'}</dd>
			</div>
			<div>
				<dt>First name</dt>
				<dd>{profile.first_name}</dd>
			</div>
			<div>
				<dt>Second name</dt>
				<dd>{profile.second_name || 'Not specified'}</dd>
			</div>
			<div>
				<dt>Last name</dt>
				<dd>{profile.last_name}</dd>
			</div>
			<div>
				<dt>Second last name</dt>
				<dd>{profile.second_last_name || 'Not specified'}</dd>
			</div>
			<div>
				<dt>Country</dt>
				<dd>{profile.country || 'Not specified'}</dd>
			</div>
			<div>
				<dt>Phone (unverified)</dt>
				<dd>
					{profile.calling_code
						? `+${profile.calling_code} ${profile.national_number}`
						: 'Not specified'}
				</dd>
			</div>
			<div>
				<dt>Preferred language</dt>
				<dd>
					{profile.preferred_locale === 'es'
						? 'Español'
						: profile.preferred_locale === 'en'
							? 'English'
							: 'Automatic'}
				</dd>
			</div>
			<div class="bio">
				<dt>Bio</dt>
				<dd>{profile.bio || 'Not specified'}</dd>
			</div>
		</dl>
		{#if target === 'me'}<Button
				variant="outline"
				disabled={pending || blocked}
				onclick={() => {
					languageEditing = true;
				}}>Change language</Button
			>{/if}
	{/if}
	<div class="modal-actions">
		<Button variant="outline" disabled={pending} onclick={load}>Reload profile</Button
		>{#if profile}<Button disabled={pending || blocked} onclick={edit}>Edit profile</Button
			>{#if images}<Button
					variant="outline"
					disabled={pending || blocked}
					onclick={() => {
						pictureEditing = true;
					}}>Change picture</Button
				>{/if}{/if}
	</div>
	{#if editing && profile && options}<Modal
			title="Edit profile"
			{pending}
			close={() => {
				editing = false;
			}}
			><ProfileEditor
				{profile}
				{options}
				{pending}
				{save}
				cancel={() => {
					editing = false;
				}}
			/></Modal
		>{/if}
	{#if pictureEditing && profile && images}<Modal
			title="Profile picture"
			{pending}
			close={() => {
				pictureEditing = false;
			}}
			><ImageControl
				cancel={() => {
					pictureEditing = false;
				}}
				api={images}
				path={`/api/profiles/${encodeURIComponent(target)}/picture`}
				revision={profile.revision}
				changed={() => {
					void load();
				}}
				busy={(value) => {
					pending = value;
				}}
			/></Modal
		>{/if}
</section>

{#if languageEditing && profile}<Modal
		title="Account language"
		{pending}
		close={() => {
			languageEditing = false;
		}}
		><LanguageEditor
			locale={profile.preferred_locale}
			{pending}
			save={saveLanguage}
			cancel={() => {
				languageEditing = false;
			}}
		/></Modal
	>{/if}

<style>
	.avatar {
		border-radius: 1rem;
		object-fit: cover;
		margin-top: 1rem;
	}
	.glass.profile-panel {
		width: min(100%, 54rem);
		padding: clamp(1rem, 4vw, 2.5rem);
		justify-self: center;
	}
	h1 {
		font-size: 2rem;
	}
	.profile-details {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 1.2rem;
		margin: 1.5rem 0;
	}
	dt {
		color: #a9c6b4;
		font-size: 0.85rem;
	}
	dd {
		margin: 0.25rem 0;
		overflow-wrap: anywhere;
	}
	.bio {
		grid-column: 1/-1;
	}
	.bio dd {
		white-space: pre-wrap;
	}
	@media (max-width: 600px) {
		.profile-details {
			grid-template-columns: 1fr;
		}
	}
</style>
