<script lang="ts" module>
	export interface ClientProps {
		userInfo: OidcUserInfo;
		client: ClientRegistration;
		disabled?: boolean;
		onAllow: () => Promise<void>;
		onDeny: () => Promise<void>;
	}
</script>

<script lang="ts">
	import type { ClientRegistration } from "@aicacia/lidp-client";
	import type { OidcUserInfo } from "@aicacia/oidc-client";
	import { m } from "$lib/paraglide/messages";
	import Avatar from "../../../lib/common/components/Avatar.svelte";

	let { userInfo, client, disabled, onAllow, onDeny }: ClientProps = $props();

	let loading = $state(false);

	async function onAllowInternal() {
		try {
			loading = true;
			await onAllow();
		} finally {
			loading = false;
		}
	}
	async function onDenyInternal() {
		try {
			loading = true;
			await onDeny();
		} finally {
			loading = false;
		}
	}
</script>

<div class="flex flex-col justify-center">
	<div class="flex flex-col items-center justify-center">
		{#if client.logoUri}
			<Avatar src={client.logoUri} alt={`${client.clientName}`} />
		{/if}

		<h1 class="m-0 mt-2 text-xl font-semibold">
			{client.clientName}
		</h1>
	</div>

	<p class="text-center">
		{m.authorize_wants_to_access({ username: userInfo.name })}
	</p>
</div>

<hr />

<div class="my-4 flex flex-col justify-center">
	<h2 class="text-sm font-medium">{m.authorize_requested_permissions()}</h2>

	{#if client.allowedScopes}
		{@const scopes = client.allowedScopes}

		<ul class="list-inside list-disc text-sm">
			{#each scopes as scope (scope)}
				<li>{scope}</li>
			{/each}
		</ul>
	{:else}
		<p class="text-sm italic">{m.authorize_no_scopes()}</p>
	{/if}
</div>

{#if client.policyUri || client.tosUri}
	<hr />
	<div class="mt-4 flex flex-row justify-center gap-4 text-xs">
		{#if client.policyUri}
			<a href={client.policyUri} target="_blank" rel="external"
				>{m.authorize_privacy_policy()}</a
			>
		{/if}
		{#if client.tosUri}
			<a href={client.tosUri} target="_blank" rel="external"
				>{m.authorize_terms_of_service()}</a
			>
		{/if}
	</div>
{/if}

<div class="mt-4 flex flex-row justify-center gap-4">
	<button
		type="button"
		class="btn secondary"
		disabled={disabled || loading}
		onclick={onDenyInternal}>{m.authorize_button_deny()}</button
	>
	<button
		type="button"
		class="btn primary"
		disabled={disabled || loading}
		onclick={onAllowInternal}>{m.authorize_button_allow()}</button
	>
</div>
