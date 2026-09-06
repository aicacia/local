import {
    Configuration,
    type ConfigurationParameters,
    DefaultApi,
} from "@aicacia/lidp-management-client";
import { createStorage } from "@aicacia/svelte-headless";
import { isTauri } from "@tauri-apps/api/core";
import { goto } from "$app/navigation";
import { resolve } from "$app/paths";
import { page } from "$app/state";
import { env } from "$env/dynamic/public";
import { afterSigninRedirect } from "./afterSigninRedirect.svelte";
import { loadLocalhostBaseUrl } from "./localhostBaseUrl.svelte";
import { getOidcClient } from "./oidc.svelte";

const lidpManagementApiUrl = createStorage<string | null>(
    "lidp-management-api-url",
    (isTauri() ? null : env.PUBLIC_LIDP_MANAGEMENT_BASE_URL) ?? null,
);

async function hydrateTauriManagementApiUrl(): Promise<void> {
    if (!isTauri()) {
        return;
    }

    const baseUrl = await loadLocalhostBaseUrl();
    if (baseUrl) {
        lidpManagementApiUrl.item = `${baseUrl}/lidp-management`;
    }
}

void hydrateTauriManagementApiUrl();

function readAccessToken(): string {
    const oidcClient = getOidcClient();
    if (!oidcClient) {
        return "";
    }
    return oidcClient.getStoredTokenResponse()?.access_token ?? "";
}

function readBasePath(): string | undefined {
    const basePath = lidpManagementApiUrl.item;
    return basePath === null ? undefined : basePath;
}

const defaultConfigurationParameters: ConfigurationParameters = {
    middleware: [
        {
            pre: async (context) => ({
                ...context,
                init: {
                    ...context.init,
                    mode: "cors",
                },
            }),
        },
        {
            post: async (context) => {
                if (context.response.status === 401) {
                    afterSigninRedirect.setURL(page.url);
                    await goto(resolve("/signin"));
                }
                return context.response;
            },
        },
    ],
    accessToken(): string {
        return readAccessToken();
    },
    get basePath(): string | undefined {
        return readBasePath();
    },
    get fetchApi() {
        return fetch;
    },
    credentials: "same-origin",
};

export const lidpManagementConfiguration = new Configuration(
    defaultConfigurationParameters,
);

export const lidpManagementApi = new DefaultApi(lidpManagementConfiguration);

export function setLidpManagementApiUrl(newLidpManagementApiUrl: string) {
    lidpManagementApiUrl.item = newLidpManagementApiUrl;
}

export function getLidpManagementApiUrl(): string | null {
    return lidpManagementApiUrl.item;
}

export async function validateLidpManagementApiUrl(
    basePath: string,
): Promise<boolean> {
    if (!basePath) {
        return false;
    }
    const configuration = new Configuration({
        ...defaultConfigurationParameters,
        basePath,
    });
    const api = new DefaultApi(configuration);

    try {
        const version = await api.version();
        return version.name === "lidp-management-server";
    } catch {
        return false;
    }
}
