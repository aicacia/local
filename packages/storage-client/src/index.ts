export type StorageRequest =
    | { type: "read"; path: string }
    | { type: "write"; path: string; content: number[] }
    | { type: "append"; path: string; content: number[] }
    | { type: "delete"; path: string }
    | { type: "entry"; path: string }
    | { type: "list"; path: string };

export type StorageEntry = {
    name: string;
    hash: string;
    size: number;
    local: boolean;
};

export type StorageResponse =
    | { type: "authenticated" }
    | { type: "read"; content: number[] }
    | { type: "written"; entry: StorageEntry }
    | { type: "appended"; entry: StorageEntry }
    | { type: "deleted" }
    | { type: "entry"; entry: StorageEntry }
    | { type: "listed"; entries: StorageEntry[] }
    | { type: "error"; code: "invalidRequest" | "operationFailed" };

type StorageSession = { token: string; expiresAt: number };

type FetchFunction = (
    input: URL | RequestInfo,
    init?: RequestInit,
) => Promise<Response>;

export type StorageClientOptions = {
    baseUrl: URL | string;
    bearerToken: () => string | Promise<string>;
    fetch?: FetchFunction;
};

export class StorageSocket {
    #queue = Promise.resolve();

    constructor(readonly socket: WebSocket) {}

    request(request: StorageRequest): Promise<StorageResponse> {
        const response = this.#queue.then(async () => {
            const next = receive(this.socket);
            this.socket.send(JSON.stringify({ type: "request", request }));
            return next;
        });
        this.#queue = response.then(
            () => undefined,
            () => undefined,
        );
        return response;
    }

    close(): void {
        this.socket.close();
    }
}

export class StorageClient {
    constructor(private readonly options: StorageClientOptions) {}

    async openSocket(): Promise<StorageSocket> {
        const baseUrl = parseUrl(this.options.baseUrl);
        const token = await this.options.bearerToken();
        if (!token) {
            throw new Error("Missing LIDP bearer token");
        }
        const response = await (this.options.fetch ?? fetch)(
            new URL("/storage/sessions", baseUrl),
            { method: "POST", headers: { Authorization: `Bearer ${token}` } },
        );
        if (!response.ok) {
            throw new Error(
                `Storage session request failed: ${response.status}`,
            );
        }
        const session = parseSession(await response.json());
        const socket = new WebSocket(webSocketUrl(baseUrl));
        await opened(socket);
        const authenticated = receive(socket);
        socket.send(
            JSON.stringify({ type: "authenticate", token: session.token }),
        );
        if ((await authenticated).type !== "authenticated") {
            socket.close();
            throw new Error("Storage socket authentication failed");
        }
        return new StorageSocket(socket);
    }
}

function parseUrl(value: URL | string): URL {
    const url = new URL(value);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
        throw new Error("LIDP API URL must use HTTP or HTTPS");
    }
    return url;
}

function parseSession(value: unknown): StorageSession {
    if (
        !value ||
        typeof value !== "object" ||
        typeof (value as StorageSession).token !== "string" ||
        typeof (value as StorageSession).expiresAt !== "number"
    ) {
        throw new Error("Invalid storage session response");
    }
    return value as StorageSession;
}

function webSocketUrl(baseUrl: URL): string {
    const url = new URL("/storage", baseUrl);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    return url.href;
}

function opened(socket: WebSocket): Promise<void> {
    if (socket.readyState === WebSocket.OPEN) {
        return Promise.resolve();
    }
    return new Promise((resolve, reject) => {
        const cleanup = () => {
            socket.removeEventListener("open", onOpen);
            socket.removeEventListener("error", onError);
        };
        const onOpen = () => {
            cleanup();
            resolve();
        };
        const onError = () => {
            cleanup();
            reject(new Error("Storage socket failed to open"));
        };
        socket.addEventListener("open", onOpen);
        socket.addEventListener("error", onError);
    });
}

function receive(socket: WebSocket): Promise<StorageResponse> {
    return new Promise((resolve, reject) => {
        const cleanup = () => {
            socket.removeEventListener("message", onMessage);
            socket.removeEventListener("close", onClose);
            socket.removeEventListener("error", onError);
        };
        const onMessage = (event: MessageEvent) => {
            cleanup();
            if (typeof event.data !== "string") {
                reject(
                    new Error("Storage socket returned a non-text response"),
                );
                return;
            }
            try {
                resolve(JSON.parse(event.data) as StorageResponse);
            } catch {
                reject(
                    new Error("Storage socket returned an invalid response"),
                );
            }
        };
        const onClose = () => {
            cleanup();
            reject(new Error("Storage socket closed"));
        };
        const onError = () => {
            cleanup();
            reject(new Error("Storage socket failed"));
        };
        socket.addEventListener("message", onMessage);
        socket.addEventListener("close", onClose);
        socket.addEventListener("error", onError);
    });
}
