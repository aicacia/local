<script lang="ts">
    import type { UserInfo } from "@aicacia/management-client";
    import { onMount } from "svelte";
    import { resolve } from "$app/paths";
    import { lidpManagementApi } from "$lib/common/state/lidpManagementClient.svelte";
    import { notifications } from "$lib/common/state/notifications.svelte";

    const limit = 25;

    let loading = $state(false);
    let error = $state<string | null>(null);
    let users = $state<UserInfo[]>([]);
    let offset = $state(0);

    async function loadUsers() {
        loading = true;
        error = null;

        try {
            users = await lidpManagementApi.listUsers({ offset, limit });
        } catch (cause) {
            console.error(cause);
            error =
                cause instanceof Error ? cause.message : "Failed to load users";
            notifications.add("Failed to load users", "error");
        } finally {
            loading = false;
        }
    }

    async function onPrev() {
        if (offset < limit || loading) {
            return;
        }
        offset -= limit;
        await loadUsers();
    }

    async function onNext() {
        if (loading) {
            return;
        }
        offset += limit;
        await loadUsers();
    }

    onMount(() => {
        void loadUsers();
    });
</script>

<div class="flex flex-col gap-4">
    <div class="flex items-center justify-between">
        <h1 class="mb-0 text-4xl">Users</h1>
        <p class="mb-0 text-sm opacity-70">Offset: {offset}</p>
    </div>

    {#if error}
        <div class="card border border-red-700 text-red-200">{error}</div>
    {:else if loading}
        <div class="card">Loading users...</div>
    {:else if users.length === 0}
        <div class="card">No users found.</div>
    {:else}
        <div class="card secondary overflow-x-auto p-0">
            <table class="min-w-full border-collapse text-left">
                <thead>
                    <tr class="border-b border-gray-300 dark:border-gray-700">
                        <th class="px-4 py-3">User ID (sub)</th>
                        <th class="px-4 py-3">Name</th>
                        <th class="px-4 py-3">Email</th>
                        <th class="px-4 py-3">Updated</th>
                    </tr>
                </thead>
                <tbody>
                    {#each users as user (user.sub)}
                        {@const parsedSub = Number(user.sub)}
                        <tr
                            class="border-b border-gray-200 align-top last:border-b-0 dark:border-gray-800"
                        >
                            <td class="px-4 py-3">
                                {#if Number.isFinite(parsedSub)}
                                    <a href={resolve(`/users/${parsedSub}`)}
                                        >{user.sub}</a
                                    >
                                {:else}
                                    <span class="disabled">{user.sub}</span>
                                {/if}
                            </td>
                            <td class="px-4 py-3"
                                >{user.name ?? user.givenName ?? "-"}</td
                            >
                            <td class="px-4 py-3">{user.email ?? "-"}</td>
                            <td class="px-4 py-3"
                                >{user.updatedAt.toLocaleString()}</td
                            >
                        </tr>
                    {/each}
                </tbody>
            </table>
        </div>
    {/if}

    <div class="flex items-center justify-end gap-2">
        <button
            type="button"
            class="btn secondary"
            onclick={onPrev}
            disabled={loading || offset === 0}>Prev</button
        >
        <button
            type="button"
            class="btn secondary"
            onclick={onNext}
            disabled={loading}>Next</button
        >
    </div>
</div>
