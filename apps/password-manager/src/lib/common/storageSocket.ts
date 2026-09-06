import { createNativeFetch } from "@aicacia/native-fetch";
import {
    StorageClient,
    type StorageEntry,
    type StorageRequest,
    type StorageResponse,
    type StorageSocket,
} from "@aicacia/storage-client";
import { isTauri } from "@tauri-apps/api/core";
import { getLidpApiUrl } from "./state/lidpClient.svelte";
import { getOidcClient } from "./state/oidc.svelte";

export type { StorageEntry, StorageRequest, StorageResponse, StorageSocket };

type FetchFunction = (
    input: URL | RequestInfo,
    init?: RequestInit,
) => Promise<Response>;

export async function openStorageSocket(): Promise<StorageSocket> {
    const { baseUrl, request } = await storageEndpoint();
    return new StorageClient({
        baseUrl,
        bearerToken: () => getOidcClient().getAccessToken(),
        fetch: request,
    }).openSocket();
}

async function storageEndpoint(): Promise<{
    baseUrl: URL;
    request: FetchFunction;
}> {
    if (isTauri()) {
        const request = createNativeFetch("lidp://app");
        const response = await request("/server-base-url");
        if (!response.ok) {
            throw new Error("LIDP local server is unavailable");
        }
        const baseUrl = parseBaseUrl(await response.json());
        return { baseUrl, request };
    }

    const configuredUrl = getLidpApiUrl();
    if (!configuredUrl) {
        throw new Error("LIDP API URL is not configured");
    }
    return { baseUrl: parseUrl(configuredUrl), request: fetch };
}

function parseBaseUrl(value: unknown): URL {
    if (
        !value ||
        typeof value !== "object" ||
        typeof (value as { baseUrl?: unknown }).baseUrl !== "string"
    ) {
        throw new Error("Invalid LIDP local server response");
    }
    return parseUrl((value as { baseUrl: string }).baseUrl);
}

function parseUrl(value: string): URL {
    const url = new URL(value);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
        throw new Error("LIDP API URL must use HTTP or HTTPS");
    }
    return url;
}
