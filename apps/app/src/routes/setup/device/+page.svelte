<script lang="ts">
    import { goto } from "$app/navigation";
    import { onMount } from "svelte";
    import {
        completeDeviceSetup,
        getDeviceResidency,
        setDeviceResidency,
        type SetupResidency,
    } from "$lib/common/state/setupClient.svelte";

    let residency = $state<SetupResidency>("passthrough");
    let error = $state("");
    let loading = $state(true);
    let submitting = $state(false);

    onMount(async () => {
        try {
            residency = await getDeviceResidency();
        } catch (reason) {
            error =
                reason instanceof Error
                    ? reason.message
                    : "Could not read device storage settings";
        } finally {
            loading = false;
        }
    });

    async function submit() {
        error = "";
        submitting = true;
        try {
            await setDeviceResidency(residency);
            await completeDeviceSetup();
            await goto("/signin");
        } catch (reason) {
            error =
                reason instanceof Error
                    ? reason.message
                    : "Could not complete device setup";
        } finally {
            submitting = false;
        }
    }
</script>

<div class="flex grow flex-col items-center justify-center">
    <div class="card w-sm">
        <h1>Choose device storage</h1>
        <p>
            This default applies to application storage unless a more-specific
            namespace, folder, or file rule overrides it.
        </p>
        {#if !loading}
            <fieldset class="flex flex-col">
                <legend>Default residency</legend>
                <label>
                    <input
                        bind:group={residency}
                        type="radio"
                        value="passthrough"
                    />
                    Passthrough — keep content on another online device.
                </label>
                <label>
                    <input bind:group={residency} type="radio" value="full" />
                    Full — store application content on this device.
                </label>
            </fieldset>
        {/if}
        {#if error}
            <p role="alert">{error}</p>
        {/if}
        <button
            class="btn primary mt-4"
            disabled={loading || submitting}
            onclick={submit}
            type="button"
        >
            {submitting ? "Saving…" : "Continue"}
        </button>
    </div>
</div>
