import { createNativeFetch } from "@aicacia/native-fetch";
import { StorageClient } from "@aicacia/storage-client";
import { createStorage } from "@aicacia/svelte-headless";

export type StorageBridgeConfig = {
    storageBridgeUrl: string;
};

const storageBridgeConfig = createStorage<StorageBridgeConfig | null>(
    "storage-bridge-config",
    null,
);
const nativeFetch = createNativeFetch("lidp://app");

const storageClient = $derived.by(
    () =>
        new StorageClient({
            url: storageBridgeConfig.item?.storageBridgeUrl ?? "",
        }),
);

export function getStorageBridgeConfig(): StorageBridgeConfig | null {
    return storageBridgeConfig.item;
}

export function getStorageBridgeUrl(): string | null {
    return storageBridgeConfig.item?.storageBridgeUrl ?? null;
}

export async function loadStorageBridgeConfig(): Promise<StorageBridgeConfig | null> {
    try {
        // Get the bridge URL from the bridge-url endpoint
        const bridgeUrlResponse = await nativeFetch("/bridge-url");
        const bridgeUrlData = (await bridgeUrlResponse.json()) as {
            bridgeUrl: string;
        };

        if (bridgeUrlData.bridgeUrl) {
            storageBridgeConfig.item = {
                storageBridgeUrl: bridgeUrlData.bridgeUrl,
            };
            return storageBridgeConfig.item;
        }

        storageBridgeConfig.reset();
        return null;
    } catch (error) {
        storageBridgeConfig.reset();
        console.warn("Failed to load storage bridge config", error);
        return null;
    }
}

export function getStorageClient(): StorageClient {
    return storageClient;
}

export function resetStorageBridgeConfig(): void {
    storageBridgeConfig.reset();
}
