<script lang="ts">
import {
    getLidpApiUrl,
    setLidpApiUrl,
} from "$lib/common/state/lidpClient.svelte";
import { getOidcClient } from "$lib/common/state/oidc.svelte";

let error = $state<string | null>(null);
let remoteUrl = $state(getLidpApiUrl() ?? "");

async function signIn(authority: string): Promise<void> {
    error = null;
    const currentAuthority = getLidpApiUrl();
    setLidpApiUrl(authority);

    try {
        await getOidcClient().signin();
    } catch (cause) {
        if (currentAuthority) {
            setLidpApiUrl(currentAuthority);
        }
        error = cause instanceof Error ? cause.message : String(cause);
    }
}

async function signInWithDesktopApp(event: Event): Promise<void> {
    event.preventDefault();
    error = null;

    await signIn("lidp://app");
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
	<button class="btn primary mt-4" type="button" onclick={signInWithDesktopApp}>
		Sign in with desktop app
	</button>

	<label class="mt-4" for="remote-url">Remote identity provider URL</label>
	<input id="remote-url" bind:value={remoteUrl} type="url" placeholder="https://example.com/lidp" />
	<button class="btn primary mt-2" type="button" onclick={signInWithRemoteServer}>
		Sign in with remote server
	</button>
	{#if error}
		<p class="mt-2 text-sm text-red-600" role="alert">{error}</p>
	{/if}
</form>
