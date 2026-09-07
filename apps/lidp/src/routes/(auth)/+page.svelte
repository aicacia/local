<script lang="ts">
    import { resolve } from "$app/paths";
    import type { PageProps } from "./$types";

    let { data }: PageProps = $props();

    const userInfo = $derived(data.userInfo);
    const displayName = $derived(
        userInfo?.name ||
            userInfo?.preferredUsername ||
            userInfo?.givenName ||
            userInfo?.email ||
            "User",
    );
</script>

<div class="flex flex-col gap-4">
    <h1 class="mb-0 text-4xl">Dashboard</h1>
    <div class="card secondary">
        <p class="mb-0">Signed in as <strong>{displayName}</strong></p>
    </div>

    <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
        <a
            class="card secondary default hover:opacity-90"
            href={resolve("/applications")}
        >
            <h2 class="mb-1 text-2xl">Applications</h2>
            <p class="mb-0 text-sm opacity-80">
                Manage application settings, roles, permissions, and clients.
            </p>
        </a>
        <a
            class="card secondary default hover:opacity-90"
            href={resolve("/devices")}
        >
            <h2 class="mb-1 text-2xl">Devices</h2>
            <p class="mb-0 text-sm opacity-80">
                Approve and revoke the devices that can synchronize your files.
            </p>
        </a>
        <a
            class="card secondary default hover:opacity-90"
            href={resolve("/users")}
        >
            <h2 class="mb-1 text-2xl">Users</h2>
            <p class="mb-0 text-sm opacity-80">
                Browse and manage user records.
            </p>
        </a>
    </div>
</div>
