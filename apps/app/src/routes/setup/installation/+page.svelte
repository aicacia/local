<script lang="ts">
    import { goto } from "$app/navigation";
    import { setupJoin, setupNew } from "$lib/common/state/setupClient.svelte";

    let mode = $state<"new" | "join">("new");
    let deviceName = $state("");
    let adminUsername = $state("");
    let adminPassword = $state("");
    let endpointAddr = $state("");
    let error = $state("");

    let submitting = $state(false);

    async function submit(event: SubmitEvent) {
        event.preventDefault();
        error = "";
        submitting = true;
        try {
            if (mode === "new") {
                await setupNew({ deviceName, adminUsername, adminPassword });
                await goto("/setup/device");
            } else {
                await setupJoin({ deviceName, endpointAddr });
                await goto("/setup/device");
            }
        } catch (reason) {
            error =
                reason instanceof Error
                    ? reason.message
                    : `Could not ${mode === "new" ? "create" : "join"} the installation`;
        } finally {
            submitting = false;
        }
    }
</script>

<div class="flex grow flex-col items-center justify-center">
    <div class="card w-sm">
        <h1>Set up this device</h1>
        <fieldset class="flex flex-col">
            <legend>Setup method</legend>
            <label>
                <input bind:group={mode} type="radio" value="new" />
                Create a new installation
            </label>
            <label>
                <input bind:group={mode} type="radio" value="join" />
                Join an existing installation
            </label>
        </fieldset>
        <form class="flex flex-col" onsubmit={submit}>
            <label class="flex flex-col">
                Device name
                <input
                    bind:value={deviceName}
                    autocomplete="nickname"
                    required
                />
            </label>
            {#if mode === "new"}
                <label class="flex flex-col">
                    Admin username
                    <input
                        bind:value={adminUsername}
                        autocomplete="username"
                        required
                    />
                </label>
                <label class="flex flex-col">
                    Admin password
                    <input
                        bind:value={adminPassword}
                        autocomplete="new-password"
                        required
                        type="password"
                    />
                </label>
            {:else}
                <label class="flex flex-col">
                    Approved device endpoint address
                    <textarea bind:value={endpointAddr} required rows="4"
                    ></textarea>
                </label>
            {/if}
            {#if error}
                <p role="alert">{error}</p>
            {/if}
            <button
                class="btn primary mt-4"
                disabled={submitting}
                type="submit"
            >
                {submitting
                    ? "Saving…"
                    : mode === "new"
                      ? "Create installation"
                      : "Start joining"}
            </button>
        </form>
    </div>
</div>
