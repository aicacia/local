import { StorageClient } from "@aicacia/storage-client";

import {
    getLocalhostBaseUrlCached,
    loadLocalhostBaseUrl,
} from "./localhostBaseUrl.svelte";

function normalizeStorageUrl(url: string | null): string | null {
    return url ? url.trim().replace(/\/$/, "") : null;
}

function storageUrlFromBaseUrl(baseUrl: string): string {
    return `${baseUrl.replace(/^https:/i, "wss:")}/storage`;
}

async function fetchStorageUrl(): Promise<string | null> {
    const cachedBaseUrl = getLocalhostBaseUrlCached();
    if (cachedBaseUrl) {
        return normalizeStorageUrl(storageUrlFromBaseUrl(cachedBaseUrl));
    }

    const baseUrl = await loadLocalhostBaseUrl();
    return baseUrl ? normalizeStorageUrl(storageUrlFromBaseUrl(baseUrl)) : null;
}

export type LocalStorageConfig = {
    storageUrl: string;
};

export async function getLocalStorageConfig(): Promise<LocalStorageConfig | null> {
    const storageUrl = await fetchStorageUrl();
    return storageUrl ? { storageUrl } : null;
}

export async function getLocalStorageUrl(): Promise<string | null> {
    const localStorageConfig = await getLocalStorageConfig();
    return localStorageConfig?.storageUrl ?? null;
}

export async function getStorageClient(): Promise<StorageClient | null> {
    const storageUrl = await getLocalStorageUrl();
    return storageUrl ? StorageClient.create({ url: storageUrl }) : null;
}

export function isLocalStorageNative(url: string): boolean {
    const normalized = normalizeStorageUrl(url);
    return normalized
        ? /^wss:\/\/(127\.0\.0\.1|localhost)(:\d+)?(\/.*)?$/i.test(normalized)
        : false;
}

export async function validateLocalStorageUrl(
    baseUrl: string,
): Promise<boolean> {
    const normalizedBaseUrl = normalizeStorageUrl(baseUrl);

    if (!normalizedBaseUrl) {
        return false;
    }

    let parsedUrl: URL;

    try {
        parsedUrl = new URL(normalizedBaseUrl);
    } catch {
        return false;
    }

    const host = parsedUrl.hostname.toLowerCase();
    const allowedHosts = ["127.0.0.1", "localhost"];

    if (parsedUrl.protocol !== "wss:") {
        return false;
    }

    if (!allowedHosts.includes(host)) {
        return false;
    }

    const WebSocketImpl = globalThis.WebSocket;

    if (!WebSocketImpl) {
        return false;
    }

    return await new Promise<boolean>((resolve) => {
        const socket = new WebSocketImpl(normalizedBaseUrl);
        const timeout = setTimeout(() => {
            cleanup();
            resolve(false);
        }, 1500);

        const cleanup = () => {
            clearTimeout(timeout);
            socket.removeEventListener("open", onOpen);
            socket.removeEventListener("error", onError);
            socket.close();
        };

        const onOpen = () => {
            cleanup();
            resolve(true);
        };

        const onError = () => {
            cleanup();
            resolve(false);
        };

        socket.addEventListener("open", onOpen);
        socket.addEventListener("error", onError);
    });
}
