<script lang="ts">
    import type {
        UserApplicationRoleResponse,
        UserConsentResponse,
        UserInfo,
    } from "@aicacia/management-client";
    import { onMount } from "svelte";
    import { page } from "$app/state";
    import { lidpManagementApi } from "$lib/common/state/lidpManagementClient.svelte";
    import { notifications } from "$lib/common/state/notifications.svelte";

    const parsedUserId = Number(page.params.userId);
    const hasValidUserId = Number.isFinite(parsedUserId);

    let loading = $state(false);
    let error = $state<string | null>(null);
    let user = $state<UserInfo | null>(null);
    let userRolesLoading = $state(false);
    let userRolesError = $state<string | null>(null);
    let userRoles = $state<UserApplicationRoleResponse[]>([]);
    let consentsLoading = $state(false);
    let consentsError = $state<string | null>(null);
    let consents = $state<UserConsentResponse[]>([]);
    let revokingConsentId = $state<number | null>(null);

    async function loadUserRoles() {
        if (!hasValidUserId) {
            return;
        }

        userRolesLoading = true;
        userRolesError = null;
        try {
            userRoles = await lidpManagementApi.listUserRolesAcrossApplications(
                {
                    userId: parsedUserId,
                },
            );
        } catch (cause) {
            console.error(cause);
            userRolesError =
                cause instanceof Error
                    ? cause.message
                    : "Failed to load assigned roles";
            notifications.add("Failed to load assigned roles", "error");
        } finally {
            userRolesLoading = false;
        }
    }

    async function loadConsents() {
        if (!hasValidUserId) {
            return;
        }

        consentsLoading = true;
        consentsError = null;
        try {
            consents = await lidpManagementApi.listUserConsents({
                userId: parsedUserId,
                offset: 0,
                limit: 100,
            });
        } catch (cause) {
            console.error(cause);
            consentsError =
                cause instanceof Error
                    ? cause.message
                    : "Failed to load user consents";
            notifications.add("Failed to load user consents", "error");
        } finally {
            consentsLoading = false;
        }
    }

    async function loadRelatedData() {
        await Promise.all([loadUserRoles(), loadConsents()]);
    }

    async function loadUser() {
        if (!hasValidUserId) {
            error = "Invalid user ID";
            return;
        }

        loading = true;
        error = null;
        try {
            user = await lidpManagementApi.getUser({
                userId: parsedUserId,
            });
            void loadRelatedData();
        } catch (cause) {
            console.error(cause);
            error =
                cause instanceof Error ? cause.message : "Failed to load user";
            notifications.add("Failed to load user", "error");
        } finally {
            loading = false;
        }
    }

    async function onRevokeConsent(consent: UserConsentResponse) {
        if (!hasValidUserId) {
            return;
        }

        if (!confirm(`Revoke consent for ${consent.clientId}?`)) {
            return;
        }

        revokingConsentId = consent.id;
        try {
            await lidpManagementApi.revokeUserConsent({
                userId: parsedUserId,
                consentId: consent.id,
            });
            notifications.add("Consent revoked", "success");
            await loadConsents();
        } catch (cause) {
            console.error(cause);
            notifications.add("Failed to revoke consent", "error");
        } finally {
            revokingConsentId = null;
        }
    }

    onMount(() => {
        void loadUser();
    });
</script>

<div class="flex flex-col gap-4">
    <h1 class="mb-0 text-4xl">User Detail</h1>

    {#if error}
        <div class="card border border-red-700 text-red-200">{error}</div>
    {:else if loading}
        <div class="card">Loading user...</div>
    {:else if user}
        <div class="flex flex-col gap-4">
            <div class="card secondary flex flex-col gap-3">
                <h2 class="mb-0 text-2xl">Profile</h2>
                <p class="mb-0"><strong>Sub:</strong> {user.sub}</p>
                <p class="mb-0"><strong>Name:</strong> {user.name ?? "-"}</p>
                <p class="mb-0">
                    <strong>Given name:</strong>
                    {user.givenName ?? "-"}
                </p>
                <p class="mb-0">
                    <strong>Family name:</strong>
                    {user.familyName ?? "-"}
                </p>
                <p class="mb-0"><strong>Email:</strong> {user.email ?? "-"}</p>
                <p class="mb-0">
                    <strong>Locale:</strong>
                    {user.locale ?? "-"}
                </p>
            </div>

            <div class="card secondary flex flex-col gap-3">
                <h2 class="mb-0 text-2xl">Roles across applications</h2>
                {#if userRolesError}
                    <div class="card border border-red-700 text-red-200">
                        {userRolesError}
                    </div>
                {:else if userRolesLoading}
                    <div class="card">Loading roles...</div>
                {:else if userRoles.length === 0}
                    <div class="card">No roles assigned.</div>
                {:else}
                    <div class="overflow-x-auto">
                        <table class="min-w-full border-collapse text-left">
                            <thead>
                                <tr
                                    class="border-b border-gray-300 dark:border-gray-700"
                                >
                                    <th class="px-4 py-3">Application</th>
                                    <th class="px-4 py-3">Role ID</th>
                                    <th class="px-4 py-3">Name</th>
                                    <th class="px-4 py-3">Description</th>
                                </tr>
                            </thead>
                            <tbody>
                                {#each userRoles as userRole (`${userRole.applicationId}-${userRole.roleId}`)}
                                    <tr
                                        class="border-b border-gray-200 align-top last:border-b-0 dark:border-gray-800"
                                    >
                                        <td class="px-4 py-3"
                                            >{userRole.applicationId}</td
                                        >
                                        <td class="px-4 py-3"
                                            >{userRole.roleId}</td
                                        >
                                        <td class="px-4 py-3"
                                            >{userRole.roleName}</td
                                        >
                                        <td class="px-4 py-3"
                                            >{userRole.roleDescription ??
                                                "-"}</td
                                        >
                                    </tr>
                                {/each}
                            </tbody>
                        </table>
                    </div>
                {/if}
            </div>

            <div class="card secondary flex flex-col gap-3">
                <h2 class="mb-0 text-2xl">Consents</h2>
                {#if consentsError}
                    <div class="card border border-red-700 text-red-200">
                        {consentsError}
                    </div>
                {:else if consentsLoading}
                    <div class="card">Loading consents...</div>
                {:else if consents.length === 0}
                    <div class="card">No consents found.</div>
                {:else}
                    <div class="overflow-x-auto">
                        <table class="min-w-full border-collapse text-left">
                            <thead>
                                <tr
                                    class="border-b border-gray-300 dark:border-gray-700"
                                >
                                    <th class="px-4 py-3">Client ID</th>
                                    <th class="px-4 py-3">Scope</th>
                                    <th class="px-4 py-3">Redirect URI</th>
                                    <th class="px-4 py-3">Actions</th>
                                </tr>
                            </thead>
                            <tbody>
                                {#each consents as consent (consent.id)}
                                    <tr
                                        class="border-b border-gray-200 align-top last:border-b-0 dark:border-gray-800"
                                    >
                                        <td class="px-4 py-3"
                                            >{consent.clientId}</td
                                        >
                                        <td class="px-4 py-3"
                                            >{consent.scope}</td
                                        >
                                        <td class="px-4 py-3 break-all"
                                            >{consent.redirectUri}</td
                                        >
                                        <td class="px-4 py-3">
                                            <button
                                                type="button"
                                                class="btn danger"
                                                onclick={() =>
                                                    onRevokeConsent(consent)}
                                                disabled={revokingConsentId !==
                                                    null}
                                            >
                                                {revokingConsentId ===
                                                consent.id
                                                    ? "Revoking..."
                                                    : "Revoke"}
                                            </button>
                                        </td>
                                    </tr>
                                {/each}
                            </tbody>
                        </table>
                    </div>
                {/if}
            </div>
        </div>
    {/if}
</div>
