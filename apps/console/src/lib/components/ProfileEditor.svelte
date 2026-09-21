<script lang="ts">
	import { untrack } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import { fields, type Profile, type Fields, type Options } from '$lib/profiles';
	let {
		profile,
		options,
		pending,
		save,
		cancel
	}: {
		profile: Profile;
		options: Options;
		pending: boolean;
		save: (fields: Fields) => void;
		cancel: () => void;
	} = $props();
	let draft = $state(untrack(() => fields(profile)));
</script>

<form
	onsubmit={(event) => {
		event.preventDefault();
		save(fields(draft));
	}}
>
	<fieldset disabled={pending}>
		<div class="profile-grid">
			<label
				>First name<input required autocomplete="given-name" bind:value={draft.first_name} /></label
			>
			<label
				>Second name (optional)<input
					autocomplete="additional-name"
					bind:value={draft.second_name}
				/></label
			>
			<label
				>Last name<input required autocomplete="family-name" bind:value={draft.last_name} /></label
			>
			<label>Second last name (optional)<input bind:value={draft.second_last_name} /></label>
			<div>
				<label for="profile-country">Country (optional)</label>
				<select id="profile-country" bind:value={draft.country}>
					<option value="">Not specified</option>
					{#each options.countries as country (country.code)}<option value={country.code}
							>{country.name}</option
						>{/each}
				</select>
			</div>
			<div>
				<label for="profile-calling-code">Calling code (optional)</label>
				<select id="profile-calling-code" bind:value={draft.calling_code}>
					<option value="">Not specified</option>
					{#each options.calling_codes as code (code)}<option value={code}>+{code}</option>{/each}
				</select>
			</div>
			<label
				>National phone number (optional)<input
					inputmode="numeric"
					autocomplete="tel-national"
					bind:value={draft.national_number}
					aria-describedby="phone-guidance"
				/></label
			>
		</div>
		<p id="phone-guidance">
			Choose a calling code and enter the national number using digits only. Preserve meaningful
			leading zeros. Phone numbers are unverified contact information.
		</p>
		<label
			>Bio (optional)<textarea rows="7" bind:value={draft.bio} aria-describedby="bio-guidance"
			></textarea></label
		>
		<p id="bio-guidance">{[...draft.bio].length} / 2,000 characters. Plain text only.</p>
		<div class="modal-actions">
			<Button variant="outline" type="button" onclick={cancel}>Cancel</Button><Button type="submit"
				>{pending ? 'Saving…' : 'Save profile'}</Button
			>
		</div>
	</fieldset>
</form>

<style>
	fieldset {
		border: 0;
		padding: 0;
		min-width: 0;
	}
	.profile-grid {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 1rem;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		margin-bottom: 0.8rem;
	}
	input,
	select,
	textarea {
		width: 100%;
		min-width: 0;
		background: #08140f;
		border: 1px solid #416150;
		color: #e9f4ec;
		border-radius: 0.6rem;
		padding: 0.65rem;
		font: inherit;
	}
	p {
		font-size: 0.85rem;
		color: #b4c7ba;
	}
	textarea {
		resize: vertical;
	}
	@media (max-width: 600px) {
		.profile-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
