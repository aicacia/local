<script lang="ts">
	import { m } from "$lib/paraglide/messages";
	import SignIn from "./_SignIn.svelte";
</script>

<div class="flex grow flex-col items-center justify-center">
	<div class="card w-sm">
		<h1>{m.sign_in()}</h1>
		<SignIn />
	</div>
</div>
