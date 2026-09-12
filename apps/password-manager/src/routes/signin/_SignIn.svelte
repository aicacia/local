<script lang="ts">
import {
    getIdpApiUrl,
    setIdpApiUrl,
} from "$lib/common/state/idpClient.svelte";
import { getOidcClient } from "$lib/common/state/oidc.svelte";

let error = $state<string | null>(null);
let remoteUrl = $state(getIdpApiUrl() ?? "");

async function signIn(authority: string): Promise<void> {
    error = null;
    const currentAuthority = getIdpApiUrl();
    setIdpApiUrl(authority);

    try {
        await getOidcClient().signin();
    } catch (cause) {
        if (currentAuthority) {
            setIdpApiUrl(currentAuthority);
        }
        error = cause instanceof Error ? cause.message : String(cause);
    }
}

async function signInWithRemoteServer(event: Event): Promise<void> {
    event.preventDefault();
    const authority = remoteUrl.trim();
    if (!authority) {
        error = "Enter the remote identity provider URL to continue.";
        return;
    }
    await signIn(authority);
}
</script>

<form class="flex flex-col">
	<label for="remote-url">LIDP server URL</label>
	<input id="remote-url" bind:value={remoteUrl} required type="url" placeholder="https://example.com/lidp" />
	<button class="btn primary mt-2" type="button" onclick={signInWithRemoteServer}>
		Sign in
	</button>
	{#if error}
		<p class="mt-2 text-sm text-red-600" role="alert">{error}</p>
	{/if}
</form>
