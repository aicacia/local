import {
    StorageClient,
    type StorageEntry,
    type StorageRequest,
    type StorageResponse,
    type StorageSocket,
} from "@aicacia/storage-client";

import { getLidpApiUrl } from "./state/lidpClient.svelte";
import { getOidcClient } from "./state/oidc.svelte";

export type { StorageEntry, StorageRequest, StorageResponse, StorageSocket };

export async function openStorageSocket(): Promise<StorageSocket> {
    return new StorageClient({
        baseUrl: storageEndpoint(),
        bearerToken: () => getOidcClient().getAccessToken(),
    }).openSocket();
}

function storageEndpoint(): URL {
    const configuredUrl = getLidpApiUrl();
    if (!configuredUrl) {
        throw new Error("LIDP API URL is not configured");
    }
    return parseUrl(configuredUrl);
}

function parseUrl(value: string): URL {
    const url = new URL(value);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
        throw new Error("LIDP API URL must use HTTP or HTTPS");
    }
    return url;
}
