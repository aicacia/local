<script lang="ts">
	import { handleNativeFetchCallback } from '@aicacia/native-fetch';
	import { onMount } from 'svelte';
	import { page } from '$app/state';

	let callbackError = $state('');

	onMount(() => {
		try {
			handleNativeFetchCallback(page.url.searchParams);
		} catch (error) {
			callbackError = error instanceof Error ? error.message : String(error);
		}
	});
</script>

<div class="flex min-h-screen items-center justify-center">
	{#if callbackError}
		<div class="max-w-xl space-y-2 px-6 text-center">
			<p class="font-semibold">Native Callback failed</p>
			<p class="text-sm wrap-break-word opacity-80">{callbackError}</p>
		</div>
	{:else}
		<p>Processing native callback...</p>
	{/if}
</div>
