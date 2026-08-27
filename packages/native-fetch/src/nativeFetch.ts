import { generateState } from "./generateState.js";
import { openUrl } from "./openUrl.js";

export type NativeFetchInit = RequestInit & {
    callbackUrl?: string;
    channelName?: string;
    timeout?: number;
};

export type CreateNativeFetchOptions = {
    requestBaseUrl?: string | URL;
};

export type HandleNativeFetchCallbackOptions = {
    channelName?: string;
};

export const NATIVE_FETCH_CHANNEL_NAME = "native-fetch";
export const NATIVE_FETCH_RESPONSE_EVENT = "native-fetch-response";

export type NativeRequestJSON = {
    url: string;
    headers: Record<string, string>;
    method: string;
    body: string | null;
    state: string;
    callbackUrl: string;
};

export type NativeResponseJSON = {
    headers: Record<string, string>;
    status: number;
    statusText: string;
    body: string | null;
    state: string;
};

/** Alias for {@link NativeRequestJSON}. */
export type NativeRequest = NativeRequestJSON;

/** Alias for {@link NativeResponseJSON}. */
export type NativeResponse = NativeResponseJSON;

async function bodyInitToString(
    body: BodyInit | null | undefined,
): Promise<string | null> {
    if (body == null) {
        return null;
    }
    if (typeof body === "string") {
        return body;
    }
    return new Response(body).text();
}

function mergeHeaders(
    requestHeaders: HeadersInit | undefined,
    initHeaders: HeadersInit | undefined,
): Record<string, string> {
    const headers = new Headers(requestHeaders);
    if (initHeaders) {
        const nextHeaders = new Headers(initHeaders);
        for (const [key, value] of nextHeaders.entries()) {
            headers.set(key, value);
        }
    }
    return Object.fromEntries(headers);
}

function inputLooksLikeHost(input: string): boolean {
    return /^(localhost|(\d{1,3}\.){3}\d{1,3}|[a-zA-Z0-9-]+(\.[a-zA-Z0-9-]+)+)(:\d+)?([/?#]|$)/.test(
        input,
    );
}

function toBaseUrl(baseUrl: string | URL): URL {
    return new URL(baseUrl instanceof URL ? baseUrl.href : baseUrl);
}

function isSameOrigin(left: URL, right: URL): boolean {
    return left.origin === right.origin;
}

function resolveRequestUrl(input: string | URL | Request, baseUrl: URL): URL {
    if (input instanceof URL) {
        return new URL(input.href);
    }
    if (input instanceof Request) {
        return new URL(input.url, baseUrl);
    }
    try {
        return new URL(input);
    } catch {
        if (input.startsWith("//")) {
            return new URL(`https:${input}`);
        }
        if (inputLooksLikeHost(input)) {
            return new URL(`https://${input}`);
        }
        return new URL(input, baseUrl);
    }
}

export function toNativeRequestUrl(
    nativeUri: string | URL,
    input: string | URL | Request,
    requestBaseUrl: string | URL,
): URL {
    const nativeBaseUrl = toBaseUrl(nativeUri);
    const baseUrl = toBaseUrl(requestBaseUrl);
    const requestUrl = resolveRequestUrl(input, baseUrl);
    const nativeRequestUrl = new URL(nativeBaseUrl.href);
    const pathname = isSameOrigin(requestUrl, nativeBaseUrl)
        ? nativeBaseUrl.pathname || "/"
        : requestUrl.pathname || "/";

    nativeRequestUrl.pathname = pathname;
    nativeRequestUrl.search = requestUrl.search;
    nativeRequestUrl.hash = requestUrl.hash;

    return nativeRequestUrl;
}

async function requestBodyToString(request: Request): Promise<string | null> {
    if (!request.body) {
        return null;
    }
    return request.clone().text();
}

export type NativeFetch = (
    input: URL | RequestInfo,
    init?: NativeFetchInit,
) => Promise<Response>;

type NativeFetchImplementation = (
    input: string | URL | Request,
    init?: NativeFetchInit,
) => Promise<Response>;

/**
 * Opens a native protocol URL and waits for the native app to respond
 * by opening a callback URL with the response data.
 */
export function createNativeFetch(
    nativeUri: string | URL,
    options: CreateNativeFetchOptions = {},
): NativeFetch {
    const requestBaseUrl =
        options.requestBaseUrl ??
        (typeof window !== "undefined"
            ? window.location.origin
            : "http://localhost");

    const implementation: NativeFetchImplementation = async (
        input,
        init,
    ): Promise<Response> => {
        const request = input instanceof Request ? input : undefined;
        const baseUrl = toBaseUrl(requestBaseUrl);
        const requestUrl = resolveRequestUrl(input, baseUrl);
        const url = toNativeRequestUrl(nativeUri, requestUrl, baseUrl);
        const originUrl =
            typeof window !== "undefined"
                ? window.location.origin
                : "http://localhost";
        const state = generateState();
        const callbackUrl = init?.callbackUrl ?? `${originUrl}/native-callback`;
        const timeout = init?.timeout;
        const channelName = init?.channelName ?? NATIVE_FETCH_CHANNEL_NAME;
        const body =
            init?.body !== undefined
                ? await bodyInitToString(init.body)
                : request
                  ? await requestBodyToString(request)
                  : null;

        const native: NativeRequestJSON = {
            url: requestUrl.href,
            headers: mergeHeaders(request?.headers, init?.headers),
            method: init?.method ?? request?.method ?? "GET",
            body,
            state,
            callbackUrl,
        };
        url.searchParams.set("native", JSON.stringify(native));

        return new Promise<Response>((resolve, reject) => {
            let popupWindow: Window | null = null;
            let timeoutId: ReturnType<typeof setTimeout> | null = null;
            let responseChannel: BroadcastChannel | null = null;
            let channelListener: ((event: MessageEvent) => void) | null = null;

            function cleanup() {
                if (timeoutId) {
                    clearTimeout(timeoutId);
                }
                if (responseChannel && channelListener) {
                    responseChannel.removeEventListener(
                        "message",
                        channelListener,
                    );
                }
                if (responseChannel) {
                    responseChannel.close();
                }
                if (popupWindow && !popupWindow.closed) {
                    popupWindow.close();
                }
            }

            if (timeout) {
                timeoutId = setTimeout(() => {
                    cleanup();
                    reject(
                        new Error(`Native fetch timeout after ${timeout}ms`),
                    );
                }, timeout);
            }

            responseChannel = new BroadcastChannel(channelName);

            channelListener = (event: MessageEvent) => {
                if (event.data?.type !== NATIVE_FETCH_RESPONSE_EVENT) {
                    return;
                }
                const nativeResponse = event.data.data as
                    | NativeResponseJSON
                    | undefined;
                if (nativeResponse?.state !== state) {
                    return;
                }
                cleanup();
                resolve(
                    new Response(nativeResponse.body, {
                        headers: nativeResponse.headers,
                        status: nativeResponse.status,
                        statusText: nativeResponse.statusText,
                    }),
                );
            };

            responseChannel.addEventListener("message", channelListener);

            popupWindow = openUrl(url, {
                popup: true,
            });
        });
    };

    return implementation as NativeFetch;
}

/**
 * Helper to handle native fetch callback in the callback page.
 * Call this in your callback route to send the response back to the waiting fetch.
 */
export function handleNativeFetchCallback(
    searchParams: URLSearchParams,
    {
        channelName = NATIVE_FETCH_CHANNEL_NAME,
    }: HandleNativeFetchCallbackOptions = {},
): void {
    const native = searchParams.get("native");
    if (!native) {
        console.warn("No native parameter in fetch callback");
        return;
    }
    const responseChannel = new BroadcastChannel(channelName);

    responseChannel.postMessage({
        type: NATIVE_FETCH_RESPONSE_EVENT,
        data: JSON.parse(native),
    });
    responseChannel.close();

    if (window.history.length > 1) {
        window.history.back();
    }
    window.close();
}

export async function handleNativeCallbackRequestUrl(
    requestUrlOrString: URL | string,
    callback: (request: Request) => Response | Promise<Response>,
): Promise<URL> {
    const requestUrl = new URL(requestUrlOrString);
    const nativeRequestParam = requestUrl.searchParams.get("native");
    if (!nativeRequestParam) {
        throw new Error("Missing `native` parameter");
    }
    const nativeRequest = JSON.parse(nativeRequestParam) as NativeRequestJSON;
    return handleNativeCallbackRequest(nativeRequest, callback);
}

export async function handleNativeCallbackRequest(
    nativeRequest: NativeRequestJSON,
    callback: (request: Request) => Response | Promise<Response>,
): Promise<URL> {
    const request = new Request(nativeRequest.url, {
        method: nativeRequest.method,
        headers: nativeRequest.headers,
        body: nativeRequest.body,
    });
    let nativeResponse: NativeResponseJSON;
    try {
        const response = await callback(request);
        nativeResponse = {
            headers: response.headers
                ? Object.fromEntries(response.headers)
                : {},
            status: response.status,
            statusText: response.statusText,
            body: await response.text(),
            state: nativeRequest.state,
        };
    } catch (error) {
        nativeResponse = {
            headers: {},
            status: 500,
            statusText: (error as Error).message,
            body: (error as Error).message,
            state: nativeRequest.state,
        };
    }

    const callbackUrl = new URL(nativeRequest.callbackUrl);
    callbackUrl.searchParams.set("native", JSON.stringify(nativeResponse));
    return callbackUrl;
}
