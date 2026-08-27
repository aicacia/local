import { expect, test } from "vitest";
import { isNativeProtocol } from "./isNativeProtocol.js";
import {
    handleNativeCallbackRequest,
    handleNativeCallbackRequestUrl,
    toNativeRequestUrl,
} from "./nativeFetch.js";

test("isNativeProtocol treats custom schemes as native", () => {
    expect(isNativeProtocol(new URL("lidp://app/config"))).toBe(true);
    expect(isNativeProtocol(new URL("https://example.com"))).toBe(false);
});

test("handleNativeCallbackRequest builds callback URL with response payload", async () => {
    const callbackUrl = await handleNativeCallbackRequest(
        {
            url: "lidp://app/config",
            headers: { accept: "application/json" },
            method: "GET",
            body: null,
            state: "abc123",
            callbackUrl: "https://app.example/native-callback",
        },
        () =>
            new Response(JSON.stringify({ ok: true }), {
                headers: { "content-type": "application/json" },
            }),
    );

    expect(callbackUrl.origin).toBe("https://app.example");
    expect(callbackUrl.pathname).toBe("/native-callback");

    const native = callbackUrl.searchParams.get("native");
    expect(native).toBeTruthy();
    if (!native) {
        return;
    }

    const response = JSON.parse(native) as {
        status: number;
        state: string;
        body: string;
    };
    expect(response.status).toBe(200);
    expect(response.state).toBe("abc123");
    expect(JSON.parse(response.body)).toEqual({ ok: true });
});

test("handleNativeCallbackRequestUrl parses native request from deep link", async () => {
    const request = {
        url: "lidp://app/config",
        headers: {},
        method: "GET",
        body: null,
        state: "state-1",
        callbackUrl: "https://app.example/native-callback",
    };
    const deepLink = new URL("lidp://app/config");
    deepLink.searchParams.set("native", JSON.stringify(request));

    const callbackUrl = await handleNativeCallbackRequestUrl(
        deepLink,
        () => new Response("done"),
    );

    const native = callbackUrl.searchParams.get("native");
    expect(native).toBeTruthy();
    if (!native) {
        return;
    }
    expect(JSON.parse(native).state).toBe("state-1");
});

test("handleNativeCallbackRequest maps handler errors to 500 responses", async () => {
    const callbackUrl = await handleNativeCallbackRequest(
        {
            url: "lidp://app/config",
            headers: {},
            method: "GET",
            body: null,
            state: "err-state",
            callbackUrl: "https://app.example/native-callback",
        },
        () => {
            throw new Error("boom");
        },
    );

    const nativeParam = callbackUrl.searchParams.get("native");
    expect(nativeParam).toBeTruthy();
    if (!nativeParam) {
        return;
    }

    const response = JSON.parse(nativeParam);
    expect(response.status).toBe(500);
    expect(response.statusText).toBe("boom");
    expect(response.body).toBe("boom");
    expect(response.state).toBe("err-state");
});

test("toNativeRequestUrl keeps the native app base URL for root-native requests", () => {
    const url = toNativeRequestUrl(
        "lidp://app",
        "lidp://app/.well-known/openid-configuration",
        "https://example.com",
    );

    expect(url.href).toBe("lidp://app/");
});

test("toNativeRequestUrl strips source origin and keeps path", () => {
    const url = toNativeRequestUrl(
        "app://api/native",
        "https://test.com/hello",
        "https://example.com",
    );

    expect(url.href).toBe("app://api/hello");
});

test("toNativeRequestUrl treats host-like inputs as absolute URLs", () => {
    const url = toNativeRequestUrl(
        "app://api/native",
        "test.com/hello?x=1#ok",
        "https://example.com/base",
    );

    expect(url.href).toBe("app://api/hello?x=1#ok");
});

test("toNativeRequestUrl resolves relative paths from configured base URL", () => {
    const url = toNativeRequestUrl(
        "app://api/native",
        "/hello?x=1",
        "https://example.com/base",
    );

    expect(url.href).toBe("app://api/hello?x=1");
});
