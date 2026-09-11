<script lang="ts" module>
	export interface IssueLike {
		message: string;
	}

	export interface IssuesProps {
		issues: readonly IssueLike[];
	}
</script>

<script lang="ts">
	let { issues }: IssuesProps = $props();
</script>

{#if issues.length > 0}
	<ul class="mt-2 space-y-1">
		{#each issues as issue, index (index)}
			<li class="text-sm text-red-500">{issue.message}</li>
		{/each}
	</ul>
{/if}
