var __classPrivateFieldGet = (this && this.__classPrivateFieldGet) || function (receiver, state, kind, f) {
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a getter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot read private member from an object whose class did not declare it");
    return kind === "m" ? f : kind === "a" ? f.call(receiver) : f ? f.value : state.get(receiver);
};
var __classPrivateFieldSet = (this && this.__classPrivateFieldSet) || function (receiver, state, value, kind, f) {
    if (kind === "m") throw new TypeError("Private method is not writable");
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a setter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot write private member to an object whose class did not declare it");
    return (kind === "a" ? f.call(receiver, value) : f ? f.value = value : state.set(receiver, value)), value;
};
var _StorageSocket_queue;
export class StorageSocket {
    constructor(socket) {
        this.socket = socket;
        _StorageSocket_queue.set(this, Promise.resolve());
    }
    request(request) {
        const response = __classPrivateFieldGet(this, _StorageSocket_queue, "f").then(async () => {
            const next = receive(this.socket);
            this.socket.send(JSON.stringify({ type: "request", request }));
            return next;
        });
        __classPrivateFieldSet(this, _StorageSocket_queue, response.then(() => undefined, () => undefined), "f");
        return response;
    }
    close() {
        this.socket.close();
    }
}
_StorageSocket_queue = new WeakMap();
export class StorageClient {
    constructor(options) {
        this.options = options;
    }
    async openSocket() {
        const baseUrl = parseUrl(this.options.baseUrl);
        const token = await this.options.bearerToken();
        if (!token) {
            throw new Error("Missing LIDP bearer token");
        }
        const response = await (this.options.fetch ?? fetch)(new URL("/storage/sessions", baseUrl), { method: "POST", headers: { Authorization: `Bearer ${token}` } });
        if (!response.ok) {
            throw new Error(`Storage session request failed: ${response.status}`);
        }
        const session = parseSession(await response.json());
        const socket = new WebSocket(webSocketUrl(baseUrl));
        await opened(socket);
        const authenticated = receive(socket);
        socket.send(JSON.stringify({ type: "authenticate", token: session.token }));
        if ((await authenticated).type !== "authenticated") {
            socket.close();
            throw new Error("Storage socket authentication failed");
        }
        return new StorageSocket(socket);
    }
}
function parseUrl(value) {
    const url = new URL(value);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
        throw new Error("LIDP API URL must use HTTP or HTTPS");
    }
    return url;
}
function parseSession(value) {
    if (!value ||
        typeof value !== "object" ||
        typeof value.token !== "string" ||
        typeof value.expiresAt !== "number") {
        throw new Error("Invalid storage session response");
    }
    return value;
}
function webSocketUrl(baseUrl) {
    const url = new URL("/storage", baseUrl);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    return url.href;
}
function opened(socket) {
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
function receive(socket) {
    return new Promise((resolve, reject) => {
        const cleanup = () => {
            socket.removeEventListener("message", onMessage);
            socket.removeEventListener("close", onClose);
            socket.removeEventListener("error", onError);
        };
        const onMessage = (event) => {
            cleanup();
            if (typeof event.data !== "string") {
                reject(new Error("Storage socket returned a non-text response"));
                return;
            }
            try {
                resolve(JSON.parse(event.data));
            }
            catch {
                reject(new Error("Storage socket returned an invalid response"));
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
//# sourceMappingURL=index.js.map