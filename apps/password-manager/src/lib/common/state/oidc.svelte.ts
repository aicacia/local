
import { OidcClient } from "@aicacia/oidc-client";
import { isTauri } from "@tauri-apps/api/core";
import { env } from "$env/dynamic/public";
import icon256x256Png from "$lib/assets/icon256x256.png";
import { getLidpApiUrl } from "./lidpClient.svelte";

const CLIENT_ID = isTauri() ? "password-manager-desktop" : "password-manager-web";

const oidcClient = $derived.by(() =>
     new OidcClient({
        clientConfig: {
            authority: getLidpApiUrl(),
            redirectUri: `${env.PUBLIC_URL}/callback`,
            clientId: CLIENT_ID,
            responseType: "code",
            registration: {
              application: {
                  name: 'Aicacia Password Manager',
                  uri: 'password-manager.aicacia.com'
                },
                clientId: CLIENT_ID,
                clientName: "Password Manager",
                scope: "openid profile address offline email phone storage",
                redirectUris: [`${env.PUBLIC_URL}/callback`],
                postLogoutRedirectUris: [`${env.PUBLIC_URL}/logout`],
                logoUri: `${env.PUBLIC_URL}${icon256x256Png}`,
                clientUri: `${env.PUBLIC_URL}`,
                policyUri: `${env.PUBLIC_URL}/policy`,
                tosUri: `${env.PUBLIC_URL}/terms`,
                profile: "web_application",
                clientType: "public",
                tokenEndpointAuthMethod: "none",
                grantTypes: ["authorization_code", "refresh_token"],
                responseTypes: ["code"],
                accessTokenExpiry: 3600,
                refreshTokenExpiry: 604800,
            },
          },
          disableNativeRequests: false,
        })
);

export function getOidcClient() {
    return oidcClient;
}
