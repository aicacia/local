<script lang="ts">
	import { onMount } from 'svelte';
	import { handleNativeFetchCallback } from '@aicacia/native-fetch';
	import { page } from '$app/state';

	let callbackError = '';

	onMount(() => {
		try {
			handleNativeFetchCallback(page.url.searchParams);
		} catch (err) {
			callbackError = err instanceof Error ? err.message : String(err);
		}
	});
</script>

<div class="flex min-h-screen items-center justify-center">
	{#if callbackError}
		<div class="max-w-xl space-y-2 px-6 text-center">
			<p class="font-semibold">Native Callback failed</p>
			{#if callbackError}
				<p class="text-sm wrap-break-word opacity-80">{callbackError}</p>
			{/if}
		</div>
	{:else}
		<p>Processing native callback...</p>
	{/if}
</div>
