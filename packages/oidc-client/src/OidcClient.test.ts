import { expect, test } from "vitest";
import { OidcClient } from "./OidcClient.js";
import { OidcClientError } from "./OidcClientError.js";
import type { OidcConfiguration } from "./OidcConfiguration.js";

function createClient(): OidcClient {
    return new OidcClient({
        clientConfig: {
            authority: "https://issuer.example",
            registration: {
                redirectUris: ["https://app.example/callback"],
            },
        },
    });
}

function createOidcConfiguration(
    overrides: Partial<OidcConfiguration> = {},
): OidcConfiguration {
    return {
        issuer: "https://issuer.example",
        authorization_endpoint: "https://issuer.example/oauth/authorize",
        token_endpoint: "https://issuer.example/oauth/token",
        check_session_iframe: "https://issuer.example/oauth/session",
        end_session_endpoint: "https://issuer.example/oauth/logout",
        jwks_uri: "https://issuer.example/oauth/jwks",
        response_types_supported: ["code"],
        subject_types_supported: ["public"],
        ...overrides,
    };
}

test("getUserInfo throws NO_ACCESS_TOKEN when token is missing", async () => {
    const client = createClient();

    try {
        await client.getUserInfo();
        expect(true).toBe(false);
    } catch (error) {
        expect(error).toBeInstanceOf(OidcClientError);
        if (error instanceof OidcClientError) {
            expect(error.code).toBe("NO_ACCESS_TOKEN");
        }
    }
});

test("getUserInfo throws NO_USERINFO_ENDPOINT when provider does not expose endpoint", async () => {
    const client = createClient();
    const testClient = client as unknown as {
        getStoredTokenResponse: () => { access_token?: string } | null;
        getOidcConfiguration: () => Promise<OidcConfiguration>;
    };

    testClient.getStoredTokenResponse = () => ({
        access_token: "access-token",
    });
    testClient.getOidcConfiguration = async () =>
        createOidcConfiguration({
            userinfo_endpoint: undefined,
        });

    try {
        await client.getUserInfo();
        expect(true).toBe(false);
    } catch (error) {
        expect(error).toBeInstanceOf(OidcClientError);
        if (error instanceof OidcClientError) {
            expect(error.code).toBe("NO_USERINFO_ENDPOINT");
        }
    }
});

test("handleSigninCallback rejects a callback without state", async () => {
    const client = createClient();

    await expect(
        client.handleSigninCallback(
            new URL("https://app.example/callback?code=code"),
        ),
    ).rejects.toMatchObject({ code: "MISSING_STATE" });
});

test("handleSigninCallback rejects an unknown state", async () => {
    const client = createClient();

    await expect(
        client.handleSigninCallback(
            new URL("https://app.example/callback?code=code&state=unknown"),
        ),
    ).rejects.toMatchObject({ code: "INVALID_STATE" });
});
