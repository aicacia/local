<script lang="ts">
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
    import { getOidcClient } from '$lib/common/state/oidc.svelte';

	let callbackError = '';
	let callbackErrorDescription = '';
	let callbackComplete = false;
	let popupCallback = false;

	onMount(async () => {
		const searchParams = new URLSearchParams(page.url.searchParams);
		const hashParams = new URLSearchParams(page.url.hash.replace(/^#/, ''));

		const error = searchParams.get('error') ?? hashParams.get('error');
		const errorDescription =
			searchParams.get('error_description') ?? hashParams.get('error_description') ?? '';

		if (error) {
			callbackError = error;
			callbackErrorDescription = errorDescription;
			return;
		}

		try {
			await getOidcClient().handleSigninCallback();
			popupCallback = window.opener != null && window.opener !== window;
			if (popupCallback) {
				callbackComplete = true;
				return;
			}
			await goto(resolve('/'));
		} catch (err) {
			callbackError = 'callback_error';
			callbackErrorDescription = err instanceof Error ? err.message : String(err);
		}
	});
</script>

<div class="flex min-h-screen items-center justify-center">
	{#if callbackError}
		<div class="max-w-xl space-y-2 px-6 text-center">
			<p class="font-semibold">Callback failed</p>
			<p class="wrap-break-word">{callbackError}</p>
			{#if callbackErrorDescription}
				<p class="text-sm wrap-break-word opacity-80">{callbackErrorDescription}</p>
			{/if}
		</div>
	{:else if callbackComplete && popupCallback}
		<div class="max-w-xl space-y-2 px-6 text-center">
			<p class="font-semibold">Sign in complete</p>
			<p>You can close this window and return to the app.</p>
		</div>
	{:else}
		<p>Processing callback...</p>
	{/if}
</div>
