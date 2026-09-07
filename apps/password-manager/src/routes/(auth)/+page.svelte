<script lang="ts">
import { onMount } from "svelte";
import { SecretStorage } from "$lib/collections/secrets/storage";
import {
    openStorageSocket,
    type StorageSocket,
} from "$lib/common/storageSocket";
import type { Secret } from "$lib/models/secret";

let secrets = $state<Secret[]>([]);
let loading = $state(true);
let error = $state<string | null>(null);
let editing = $state<Secret | null>(null);
let name = $state("");
let secret = $state("");
let uri = $state("");
let notes = $state("");
let favorite = $state(false);
let storage: SecretStorage | null = null;
let sortedSecrets = $derived(
    [...secrets].sort(
        (left, right) =>
            Number(right.favorite) - Number(left.favorite) ||
            left.name.localeCompare(right.name),
    ),
);

onMount(() => {
    let socket: StorageSocket | null = null;
    void (async () => {
        socket = await openStorageSocket();
        storage = new SecretStorage(socket);
        secrets = await storage.list();
    })()
        .catch((cause: unknown) => {
            error = message(cause);
        })
        .finally(() => {
            loading = false;
        });

    return () => socket?.close();
});

function newSecret(): void {
    editing = null;
    name = "";
    secret = "";
    uri = "";
    notes = "";
    favorite = false;
}

function edit(value: Secret): void {
    editing = value;
    name = value.name;
    secret = value.secret;
    uri = value.uris[0]?.uri ?? "";
    notes = value.notes ?? "";
    favorite = value.favorite;
}

async function save(): Promise<void> {
    if (!storage) {
        return;
    }
    error = null;
    const now = new Date().toISOString();
    const previous = editing;
    const value: Secret = {
        id: previous?.id ?? crypto.randomUUID(),
        name,
        secret,
        uris: uri ? [{ uri, match: "domain" }] : [],
        notes: notes || undefined,
        favorite,
        fields: previous?.fields ?? [],
        history:
            previous && previous.secret !== secret
                ? [
                      ...previous.history,
                      { secret: previous.secret, changedAt: now },
                  ]
                : (previous?.history ?? []),
        createdAt: previous?.createdAt ?? now,
        updatedAt: now,
    };

    try {
        await storage.save(value);
        secrets = [...secrets.filter((item) => item.id !== value.id), value];
        edit(value);
    } catch (cause) {
        error = message(cause);
    }
}

async function remove(value: Secret): Promise<void> {
    if (!storage || !confirm(`Delete ${value.name}?`)) {
        return;
    }
    error = null;
    try {
        await storage.delete(value.id);
        secrets = secrets.filter((item) => item.id !== value.id);
        if (editing?.id === value.id) {
            newSecret();
        }
    } catch (cause) {
        error = message(cause);
    }
}

function message(cause: unknown): string {
    return cause instanceof Error ? cause.message : String(cause);
}
</script>

<div class="mx-auto flex w-full max-w-5xl grow flex-col gap-6 overflow-auto p-6">
	<header class="flex flex-wrap items-center justify-between gap-3">
		<div>
			<h1>Password Manager</h1>
			<p class="text-gray-600 dark:text-gray-400">Your records are stored in your LIDP application scope.</p>
		</div>
		<button class="btn primary" type="button" onclick={newSecret}>New secret</button>
	</header>

	{#if error}
		<p class="rounded-lg bg-red-700 px-3 py-2 text-white" role="alert">{error}</p>
	{/if}

	<div class="grid gap-6 md:grid-cols-[minmax(16rem,1fr)_minmax(20rem,2fr)]">
		<section class="card">
			<h2>Secrets</h2>
			{#if loading}
				<p>Loading vault…</p>
			{:else if sortedSecrets.length === 0}
				<p>No secrets yet.</p>
			{:else}
				<ul class="space-y-2 p-0">
					{#each sortedSecrets as value (value.id)}
						<li class="flex items-center gap-2">
							<button
								class="btn secondary grow text-left"
								class:active={editing?.id === value.id}
								type="button"
								onclick={() => edit(value)}
							>
								{value.favorite ? '★ ' : ''}{value.name}
							</button>
							<button class="btn danger" type="button" onclick={() => remove(value)}>Delete</button>
						</li>
					{/each}
				</ul>
			{/if}
		</section>

		<form class="card flex flex-col gap-3" onsubmit={(event) => { event.preventDefault(); void save(); }}>
			<h2>{editing ? 'Edit secret' : 'New secret'}</h2>
			<label for="name">Name</label>
			<input id="name" bind:value={name} required />

			<label for="secret">Secret</label>
			<input id="secret" bind:value={secret} required type="password" />

			<label for="uri">Website</label>
			<input id="uri" bind:value={uri} type="url" />

			<label for="notes">Notes</label>
			<textarea id="notes" bind:value={notes} rows="4"></textarea>

			<label class="flex items-center gap-2" for="favorite">
				<input id="favorite" bind:checked={favorite} type="checkbox" /> Favorite
			</label>

			<div class="flex gap-2">
				<button class="btn primary" disabled={loading} type="submit">Save</button>
				<button class="btn secondary" type="button" onclick={newSecret}>Clear</button>
			</div>
		</form>
	</div>
</div>
