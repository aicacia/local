<script lang="ts" module>
	interface Props {
		notification: Notification;
	}
</script>

<script lang="ts">
	import type { Notification } from '@aicacia/svelte-headless';
	import { CircleX, CircleCheck, Info, TriangleAlert } from '@lucide/svelte';
	import { notifications } from '$lib/common/state/notifications.svelte';

	let { notification }: Props = $props();

	function onDelete() {
		notifications.remove(notification.id);
	}
</script>

<button
	class="m-1 flex grow cursor-pointer flex-row items-center px-3 py-2 shadow"
	class:bg-green-600={notification.type === 'success'}
	class:bg-red-600={notification.type === 'error'}
	class:bg-blue-600={notification.type === 'info'}
	class:bg-yellow-600={notification.type === 'warning'}
	onclick={onDelete}
>
	<div class="mr-2 h-6 w-6 text-white">
		{#if notification.type === 'error'}
			<CircleX />
		{:else if notification.type === 'success'}
			<CircleCheck />
		{:else if notification.type === 'info'}
			<Info />
		{:else if notification.type === 'warning'}
			<TriangleAlert />
		{/if}
	</div>
	<div class="grow text-left text-white">{notification.message}</div>
</button>
