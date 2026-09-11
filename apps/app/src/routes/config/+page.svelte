<script lang="ts">
    import { goto } from "$app/navigation";
    import { resolve } from "$app/paths";
    import ConfigForm from "$lib/common/components/ConfigForm.svelte";
    import { m } from "$lib/paraglide/messages";
</script>

<div class="flex grow flex-col items-center justify-center">
    <div class="card w-sm">
        <h1>{m.env_config()}</h1>
        <ConfigForm onSaved={() => void goto(resolve("/"))} />
    </div>
</div>
