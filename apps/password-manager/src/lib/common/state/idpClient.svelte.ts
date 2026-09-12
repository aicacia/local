import {
    Configuration,
    type ConfigurationParameters,
    DefaultApi,
} from "@aicacia/idp-client";

import { createStorage } from "@aicacia/svelte-headless";

import { goto } from "$app/navigation";
import { resolve } from "$app/paths";
import { page } from "$app/state";
import { env } from "$env/dynamic/public";
import { afterSigninRedirect } from "./afterSigninRedirect.svelte";
import { getOidcClient } from "./oidc.svelte";

const defaultIdpApiUrl = env.PUBLIC_LIDP_BASE_URL ?? "";
const idpApiUrl = createStorage<string>("idp-api-url", defaultIdpApiUrl);

if (!isHttpUrl(idpApiUrl.item)) {
    idpApiUrl.item = isHttpUrl(defaultIdpApiUrl) ? defaultIdpApiUrl : "";
}

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
    fetchApi: fetch,
    accessToken(_name, _scopes) {
        return getOidcClient().getStoredTokenResponse()?.access_token ?? "";
    },
    get basePath() {
        return idpApiUrl.item;
    },
    credentials: "same-origin",
};

export const lidpConfiguration = new Configuration(
    defaultConfigurationParameters,
);

export const idpApi = new DefaultApi(lidpConfiguration);

export function setIdpApiUrl(newIdpApiUrl: string) {
    if (!isHttpUrl(newIdpApiUrl)) {
        throw new Error("LIDP API URL must use HTTP or HTTPS");
    }
    idpApiUrl.item = newIdpApiUrl;
}

export function getIdpApiUrl(): string | null {
    return isHttpUrl(idpApiUrl.item) ? idpApiUrl.item : null;
}

function isHttpUrl(value: string | null | undefined): value is string {
    if (!value) {
        return false;
    }
    try {
        const url = new URL(value);
        return url.protocol === "http:" || url.protocol === "https:";
    } catch {
        return false;
    }
}

export async function validateIdpApiUrl(basePath: string): Promise<boolean> {
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
        return version.name === "idp-server";
    } catch {
        return false;
    }
}
