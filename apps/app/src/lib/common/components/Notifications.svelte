<script lang="ts">
	import { fly } from 'svelte/transition';
	import { flip } from 'svelte/animate';
	import { notifications } from '$lib/common/state/notifications.svelte';
	import Notification from './Notification.svelte';
</script>

<div class="fixed top-0 left-1/2 z-100000 -translate-x-1/2">
	<div class="flex flex-col">
		{#each notifications.items as notification (notification.id)}
			<div animate:flip in:fly={{ y: -64, duration: 300 }} out:fly={{ y: -64, duration: 300 }}>
				<Notification {notification} />
			</div>
		{/each}
	</div>
</div>
