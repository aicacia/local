import { describe, expect, it, vi } from "vitest";

import { StorageClient } from "./index";

describe("StorageClient", () => {
    it("exchanges an access token before listing filesystems", async () => {
        const fetch = vi
            .fn()
            .mockResolvedValueOnce(
                Response.json({ access_token: "storage-token" }),
            )
            .mockResolvedValueOnce(
                Response.json([{ id: "filesystem-id", name: "Vault" }]),
            );
        const client = new StorageClient({
            baseUrl: "https://idp.example",
            audience: "https://storage.example/audience",
            clientId: "password-manager-web",
            bearerToken: () => "user-token",
            fetch,
        });

        await expect(client.listFileSystems()).resolves.toEqual([
            { id: "filesystem-id", name: "Vault" },
        ]);
        const exchange = fetch.mock.calls[0];
        expect(exchange?.[0]).toEqual(
            new URL("/oauth2/token", "https://idp.example"),
        );
        const body = exchange?.[1]?.body as URLSearchParams;
        expect(body.get("grant_type")).toBe(
            "urn:ietf:params:oauth:grant-type:token-exchange",
        );
        expect(body.get("subject_token")).toBe("user-token");
        expect(body.get("audience")).toBe("https://storage.example/audience");
        expect(JSON.parse(body.get("authorization_details") ?? "null")).toEqual(
            [{ type: "storage", actions: ["read"] }],
        );
        expect(fetch.mock.calls[1]?.[1]?.headers).toEqual({
            Authorization: "Bearer storage-token",
        });
    });
});
