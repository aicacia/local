import {
    Configuration,
    type ConfigurationParameters,
    DefaultApi,
} from "@aicacia/lidp-client";
import { createNativeFetch } from "@aicacia/native-fetch";
import { createStorage } from "@aicacia/svelte-headless";
import { isTauri } from "@tauri-apps/api/core";
import { goto } from "$app/navigation";
import { resolve } from "$app/paths";
import { page } from "$app/state";
import { env } from "$env/dynamic/public";
import { afterSigninRedirect } from "./afterSigninRedirect.svelte";
import { getOidcClient } from "./oidc.svelte";

const lidpApiUrl = createStorage<string>(
    "lidp-api-url",
    (isTauri() ? "lidp://app" : env.PUBLIC_LIDP_BASE_URL) ?? "lidp://app",
);
let lidpApiIsNative = $derived.by(() => lidpApiUrl.item?.startsWith("lidp:"));

export const defaultConfigurationParameters: ConfigurationParameters = {
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
    get fetchApi() {
        return lidpApiIsNative
            ? createNativeFetch(lidpApiUrl.item ?? "lidp://app")
            : fetch;
    },
    accessToken(_name, _scopes) {
        return getOidcClient().getStoredTokenResponse()?.access_token ?? "";
    },
    get basePath() {
        return lidpApiUrl.item;
    },
    credentials: "same-origin",
};

export const lidpConfiguration = new Configuration(
    defaultConfigurationParameters,
);

export const lidpApi = new DefaultApi(lidpConfiguration);

export function setLidpApiUrl(newLidpApiUrl: string) {
    lidpApiUrl.item = newLidpApiUrl;
}
export function getLidpApiUrl(): string | null {
    return lidpApiUrl.item;
}

export function isLidpApiNative(): boolean {
    return lidpApiIsNative;
}

export async function validateLidpApiUrl(basePath: string): Promise<boolean> {
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
        return version.name === "lidp-server";
    } catch {
        return false;
    }
}
