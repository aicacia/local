<script lang="ts">
	import { m } from "$lib/paraglide/messages";
</script>

<div class="flex grow flex-col items-center justify-center px-4">
	<div class="card w-lg max-w-2xl">
		<div class="p-6">
			<h1 class="mb-3 text-2xl font-semibold">{m.policy_title()}</h1>
			<p class="text-muted-foreground mb-4 text-sm">{m.policy_summary()}</p>

			<ul class="mb-4 list-disc space-y-2 pl-5 text-sm">
				<li>{m.policy_data()}</li>
				<li>{m.policy_use()}</li>
				<li>{m.policy_sharing()}</li>
				<li>{m.policy_retention()}</li>
			</ul>
		</div>
	</div>
</div>
