import {
    type FileSystemResource,
    StorageClient,
    type StorageEntry,
    type StorageRequest,
    type StorageResponse,
    type StorageSocket,
} from "@aicacia/storage-client";

import { env } from "$env/dynamic/public";
import { getIdpApiUrl } from "./state/idpClient.svelte";
import { getOidcClient } from "./state/oidc.svelte";

export type {
    FileSystemResource,
    StorageEntry,
    StorageRequest,
    StorageResponse,
    StorageSocket,
};

function client(): StorageClient {
    return new StorageClient({
        baseUrl: storageEndpoint(),
        audience: env.PUBLIC_STORAGE_AUDIENCE || storageEndpoint().origin,
        clientId: "password-manager-web",
        bearerToken: () => getOidcClient().getAccessToken(),
    });
}

export function listFileSystems(): Promise<FileSystemResource[]> {
    return client().listFileSystems();
}

export function createFileSystem(): Promise<FileSystemResource> {
    return client().createFileSystem("Password Manager");
}

export function openStorageSocket(
    filesystemId: string,
): Promise<StorageSocket> {
    return client().openSocket(filesystemId);
}

function storageEndpoint(): URL {
    const configuredUrl = getIdpApiUrl();
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
