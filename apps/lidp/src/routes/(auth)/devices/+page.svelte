<script lang="ts">
    import { toDataURL } from "qrcode";
    import { onMount } from "svelte";
    import { page } from "$app/state";
    import {
        approveDevice,
        createDeviceInvitation,
        type DeviceInfo,
        enrollDevice,
        getDeviceApprovalPayload,
        listDevices,
        redeemDeviceInvitation,
        renameDevice,
        revokeDevice,
    } from "$lib/common/state/devices.svelte";
    import { lidpApi } from "$lib/common/state/lidpClient.svelte";
    import { notifications } from "$lib/common/state/notifications.svelte";

    interface BarcodeDetectorResult {
        rawValue?: string;
    }

    interface BarcodeDetector {
        detect(source: ImageBitmapSource): Promise<BarcodeDetectorResult[]>;
    }

    interface BarcodeDetectorConstructor {
        new (options?: { formats?: string[] }): BarcodeDetector;
    }

    let devices = $state<DeviceInfo[]>([]);
    let loading = $state(false);
    let error = $state<string | null>(null);
    let deviceName = $state("");
    let enrolling = $state(false);
    let deviceEndpointId = $state<string | null>(null);
    let pendingPrompt = $state<DeviceInfo | null>(null);
    let editingId = $state<number | null>(null);
    let editingName = $state("");
    let invitationLink = $state<string | null>(null);
    let invitationQr = $state<string | null>(null);
    let creatingInvitation = $state(false);
    let pairingName = $state("");
    let pairingState = $state<"pending" | "approved" | null>(null);
    let redeeming = $state(false);
    let scanning = $state(false);
    let scannerVideo = $state<HTMLVideoElement | undefined>(undefined);
    let scannerStream: MediaStream | null = null;
    let scannerTimer: ReturnType<typeof setInterval> | null = null;

    const initialLoading = $derived(!error && loading && devices.length === 0);
    const empty = $derived(!error && !loading && devices.length === 0);
    const hasDevices = $derived(!error && devices.length > 0);
    const bootstrapEnrollment = $derived(
        Boolean(deviceEndpointId) && devices.length === 0,
    );
    const localDeviceApproved = $derived(
        devices.some(
            (device) =>
                device.state === "approved" &&
                device.publicKey === deviceEndpointId,
        ),
    );
    const invitationId = $derived(page.url.searchParams.get("invitation"));
    const invitationSecret = $derived(page.url.searchParams.get("secret"));
    const hasPairingInvitation = $derived(
        Boolean(invitationId && invitationSecret),
    );
    const supportsScanner = $derived(
        typeof window !== "undefined" && "BarcodeDetector" in window,
    );

    function formatTimestamp(value: number): string {
        return new Date(value * 1000).toLocaleString();
    }

    function shortValue(value: string): string {
        return value.length > 20
            ? `${value.slice(0, 10)}…${value.slice(-8)}`
            : value;
    }

    function pairingLink(id: number, secret: string): string {
        const url = new URL("lidp://pair");
        url.searchParams.set("invitation", String(id));
        url.searchParams.set("secret", secret);
        return url.toString();
    }

    function isPairingLink(value: string): boolean {
        try {
            const url = new URL(value);
            return (
                url.protocol === "lidp:" &&
                url.hostname === "pair" &&
                Boolean(
                    url.searchParams.get("invitation") &&
                    url.searchParams.get("secret"),
                )
            );
        } catch {
            return false;
        }
    }

    async function loadDevices() {
        loading = true;
        error = null;
        try {
            const nextDevices = await listDevices();
            const newlyPending = nextDevices.find(
                (device) =>
                    device.state === "pending" &&
                    !devices.some((current) => current.id === device.id),
            );
            devices = nextDevices;
            if (localDeviceApproved) {
                pendingPrompt ??= newlyPending ?? null;
            }
        } catch (cause) {
            console.error(cause);
            error =
                cause instanceof Error
                    ? cause.message
                    : "Failed to load devices";
        } finally {
            loading = false;
        }
    }

    async function onEnroll(event: SubmitEvent) {
        event.preventDefault();
        if (!deviceName.trim()) {
            return;
        }

        enrolling = true;
        try {
            const device = await lidpApi.device();
            const enrollment = await enrollDevice(
                deviceName.trim(),
                device.publicKey,
                device.address,
            );
            deviceName = "";
            notifications.add(
                enrollment.state === "approved"
                    ? "Device approved"
                    : "Device is waiting for approval",
                "success",
            );
            await loadDevices();
        } catch (cause) {
            console.error(cause);
            notifications.add("Failed to enroll device", "error");
        } finally {
            enrolling = false;
        }
    }

    async function onCreateInvitation() {
        creatingInvitation = true;
        try {
            const device = await lidpApi.device();
            const invitation = await createDeviceInvitation(device.publicKey);
            invitationLink = pairingLink(invitation.id, invitation.secret);
            invitationQr = await toDataURL(invitationLink);
        } catch (cause) {
            console.error(cause);
            notifications.add("Failed to create pairing invitation", "error");
        } finally {
            creatingInvitation = false;
        }
    }

    async function onCopyInvitation() {
        if (!invitationLink) {
            return;
        }
        try {
            await navigator.clipboard.writeText(invitationLink);
            notifications.add("Pairing link copied", "success");
        } catch {
            notifications.add("Could not copy pairing link", "error");
        }
    }

    async function onRedeemInvitation(event: SubmitEvent) {
        event.preventDefault();
        if (!invitationSecret || !pairingName.trim()) {
            return;
        }

        redeeming = true;
        try {
            const device = await lidpApi.device();
            const enrollment = await redeemDeviceInvitation(
                invitationSecret,
                pairingName.trim(),
                device.publicKey,
                device.address,
            );
            pairingState =
                enrollment.state === "approved" ? "approved" : "pending";
            await loadDevices();
        } catch (cause) {
            console.error(cause);
            notifications.add("Failed to redeem pairing invitation", "error");
        } finally {
            redeeming = false;
        }
    }

    async function onApprove(device: DeviceInfo) {
        try {
            const payload = await getDeviceApprovalPayload(device.id);
            const { signature } = await lidpApi.signDeviceMessage({
                signDeviceMessage: { message: payload },
            });
            await approveDevice(device.id, signature);
            pendingPrompt = null;
            notifications.add("Device approved", "success");
            await loadDevices();
        } catch (cause) {
            console.error(cause);
            notifications.add("Failed to approve device", "error");
        }
    }

    function onStartRename(device: DeviceInfo) {
        editingId = device.id;
        editingName = device.name;
    }

    async function onRename(device: DeviceInfo) {
        if (!editingName.trim()) {
            return;
        }
        try {
            await renameDevice(device.id, editingName.trim());
            editingId = null;
            notifications.add("Device renamed", "success");
            await loadDevices();
        } catch (cause) {
            console.error(cause);
            notifications.add("Failed to rename device", "error");
        }
    }

    async function onRevoke(device: DeviceInfo) {
        if (
            !confirm(
                `Revoke ${device.name}? This stops future synchronization.`,
            )
        ) {
            return;
        }
        try {
            await revokeDevice(device.id);
            notifications.add("Device revoked", "success");
            await loadDevices();
        } catch (cause) {
            console.error(cause);
            notifications.add("Failed to revoke device", "error");
        }
    }

    function stopScanner() {
        if (scannerTimer) {
            clearInterval(scannerTimer);
            scannerTimer = null;
        }
        scannerStream?.getTracks().forEach((track) => {
            track.stop();
        });
        scannerStream = null;
        scanning = false;
    }

    async function onStartScanner() {
        const BarcodeDetector = (
            window as typeof window & {
                BarcodeDetector?: BarcodeDetectorConstructor;
            }
        ).BarcodeDetector;
        const video = scannerVideo;
        if (!BarcodeDetector || !video) {
            return;
        }

        try {
            const detector = new BarcodeDetector({ formats: ["qr_code"] });
            scannerStream = await navigator.mediaDevices.getUserMedia({
                video: { facingMode: "environment" },
            });
            video.srcObject = scannerStream;
            await video.play();
            scanning = true;
            scannerTimer = setInterval(() => {
                void detector.detect(video).then((codes) => {
                    const value = codes[0]?.rawValue;
                    if (!value || !isPairingLink(value)) {
                        return;
                    }
                    stopScanner();
                    window.location.assign(value);
                });
            }, 500);
        } catch (cause) {
            console.error(cause);
            stopScanner();
            notifications.add("Could not start the QR scanner", "error");
        }
    }

    onMount(() => {
        void lidpApi.device().then((device) => {
                deviceEndpointId = device.publicKey;
            })
            .catch(console.error);
        void loadDevices();
        const timer = setInterval(() => void loadDevices(), 5000);
        return () => {
            clearInterval(timer);
            stopScanner();
        };
    });
</script>

<div class="flex flex-col gap-4">
    <div class="flex items-center justify-between">
        <h1 class="mb-0 text-4xl">Devices</h1>
        <button
            type="button"
            class="btn secondary"
            onclick={() => void loadDevices()}
            disabled={loading}>Refresh</button
        >
    </div>

    {#if hasPairingInvitation}
        <form
            class="card secondary flex flex-col gap-3"
            onsubmit={onRedeemInvitation}
        >
            <h2 class="mb-0 text-2xl">Pair this device</h2>
            {#if pairingState === "pending"}
                <p class="mb-0">
                    Waiting for approval from an existing device.
                </p>
            {:else if pairingState === "approved"}
                <p class="mb-0">This device is approved.</p>
            {:else}
                <label class="flex flex-col gap-1">
                    <span>Device name</span>
                    <input bind:value={pairingName} type="text" required />
                </label>
                <div class="flex justify-end">
                    <button
                        type="submit"
                        class="btn primary"
                        disabled={redeeming}
                        >{redeeming ? "Pairing..." : "Pair device"}</button
                    >
                </div>
            {/if}
        </form>
    {:else if bootstrapEnrollment}
        <form class="card secondary flex flex-col gap-3" onsubmit={onEnroll}>
            <h2 class="mb-0 text-2xl">Add this device</h2>
            <label class="flex flex-col gap-1">
                <span>Device name</span>
                <input bind:value={deviceName} type="text" required />
            </label>
            <div class="flex justify-end">
                <button type="submit" class="btn primary" disabled={enrolling}
                    >{enrolling ? "Adding..." : "Add device"}</button
                >
            </div>
        </form>
    {:else}
        <div class="card secondary flex flex-col gap-3">
            <h2 class="mb-0 text-2xl">Pair a device</h2>
            {#if supportsScanner}
                <p class="mb-0">Scan a pairing QR code to open the LIdP app.</p>
                <video
                    class="max-w-full"
                    style:display={scanning ? "block" : "none"}
                    bind:this={scannerVideo}
                    muted
                    playsinline
                ></video>
                {#if scanning}
                    <button
                        type="button"
                        class="btn secondary"
                        onclick={stopScanner}>Stop scanner</button
                    >
                {:else}
                    <button
                        type="button"
                        class="btn primary"
                        onclick={() => void onStartScanner()}
                        >Scan pairing QR</button
                    >
                {/if}
            {:else}
                <p class="mb-0">Scan a pairing QR code on this device to add it.</p>
            {/if}
        </div>
    {/if}

    {#if localDeviceApproved}
        <div class="card secondary flex flex-col gap-3">
            <h2 class="mb-0 text-2xl">Pair another device</h2>
            <button
                type="button"
                class="btn primary self-start"
                onclick={() => void onCreateInvitation()}
                disabled={creatingInvitation}
                >{creatingInvitation
                    ? "Creating invitation..."
                    : "Create pairing invitation"}</button
            >
            {#if invitationQr && invitationLink}
                <img
                    class="h-64 w-64 self-start bg-white p-2"
                    src={invitationQr}
                    alt="Pairing QR code"
                />
                <code class="break-all select-all">{invitationLink}</code>
                <button
                    type="button"
                    class="btn secondary self-start"
                    onclick={() => void onCopyInvitation()}
                    >Copy pairing link</button
                >
            {/if}
        </div>
    {/if}

    {#if pendingPrompt}
        <div class="card secondary flex flex-col gap-3">
            <h2 class="mb-0 text-2xl">Approve new device?</h2>
            <p class="mb-0">{pendingPrompt.name} wants to join your devices.</p>
            <div class="flex justify-end gap-2">
                <button
                    type="button"
                    class="btn secondary"
                    onclick={() => (pendingPrompt = null)}>Not now</button
                >
                <button
                    type="button"
                    class="btn primary"
                    onclick={() => pendingPrompt && void onApprove(pendingPrompt)}
                    >Approve</button
                >
            </div>
        </div>
    {/if}

    {#if error}<div class="card border border-red-700 text-red-200">
            {error}
        </div>{/if}
    {#if initialLoading}<div class="card">Loading devices...</div>{/if}

    {#if empty}
        <div class="card secondary">
            <h2 class="mb-0 text-2xl">No devices yet</h2>
        </div>
    {/if}

    {#if hasDevices}
        <div class="card secondary overflow-x-auto p-0">
            <table class="min-w-full border-collapse text-left">
                <thead>
                    <tr class="border-b border-gray-300 dark:border-gray-700">
                        <th class="px-4 py-3">Name</th>
                        <th class="px-4 py-3">State</th>
                        <th class="px-4 py-3">Public key</th>
                        <th class="px-4 py-3">Updated</th>
                        <th class="px-4 py-3">Actions</th>
                    </tr>
                </thead>
                <tbody>
                    {#each devices as device (device.id)}
                        <tr
                            class="border-b border-gray-200 align-top last:border-b-0 dark:border-gray-800"
                        >
                            <td class="px-4 py-3">
                                {#if editingId === device.id}
                                    <form
                                        class="flex gap-2"
                                        onsubmit={(event) => {
                                            event.preventDefault();
                                            void onRename(device);
                                        }}
                                    >
                                        <input
                                            bind:value={editingName}
                                            type="text"
                                            aria-label="Device name"
                                        />
                                        <button
                                            type="submit"
                                            class="btn secondary">Save</button
                                        >
                                    </form>
                                {:else}
                                    {device.name}
                                {/if}
                            </td>
                            <td class="px-4 py-3">{device.state}</td>
                            <td class="px-4 py-3"
                                ><code title={device.publicKey}
                                    >{shortValue(device.publicKey)}</code
                                ></td
                            >
                            <td class="px-4 py-3"
                                >{formatTimestamp(device.updatedAt)}</td
                            >
                            <td class="px-4 py-3">
                                {#if device.state !== "revoked"}
                                    <div class="flex gap-2">
                                        <button
                                            type="button"
                                            class="btn secondary"
                                            onclick={() =>
                                                onStartRename(device)}
                                            >Rename</button
                                        >
                                        {#if localDeviceApproved && device.state === "pending"}
                                            <button
                                                type="button"
                                                class="btn primary"
                                                onclick={() =>
                                                    (pendingPrompt = device)}
                                                >Review</button
                                            >
                                        {/if}
                                        <button
                                            type="button"
                                            class="btn secondary"
                                            onclick={() =>
                                                void onRevoke(device)}
                                            >Revoke</button
                                        >
                                    </div>
                                {/if}
                            </td>
                        </tr>
                    {/each}
                </tbody>
            </table>
        </div>
    {/if}
</div>
