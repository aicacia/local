<script lang="ts">
	import { m } from "$lib/paraglide/messages";
</script>

<div class="flex grow flex-col items-center justify-center px-4">
	<div class="card w-lg max-w-2xl">
		<div class="p-6">
			<h1 class="mb-3 text-2xl font-semibold">{m.terms_title()}</h1>
			<p class="text-muted-foreground mb-4 text-sm">{m.terms_intro()}</p>

			<ul class="mb-4 list-disc space-y-2 pl-5 text-sm">
				<li>{m.terms_use()}</li>
				<li>{m.terms_liability()}</li>
				<li>{m.terms_changes()}</li>
			</ul>
		</div>
	</div>
</div>
