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
    async listFileSystems() {
        const token = await this.exchangeToken(["read"]);
        const response = await this.request("/storage/filesystems", token);
        if (!response.ok) {
            throw new Error(`Filesystem list failed: ${response.status}`);
        }
        return (await response.json());
    }
    async createFileSystem(name) {
        const token = await this.exchangeToken(["write"]);
        const response = await this.request("/storage/filesystems", token, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ name: name ?? null }),
        });
        if (!response.ok) {
            throw new Error(`Filesystem creation failed: ${response.status}`);
        }
        return (await response.json());
    }
    async openSocket(filesystemId) {
        if (!filesystemId) {
            throw new Error("Select a filesystem before opening storage");
        }
        const baseUrl = parseUrl(this.options.baseUrl);
        const token = await this.exchangeToken(["read", "write"]);
        const url = new URL("/storage", baseUrl);
        url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
        url.searchParams.set("access_token", token);
        url.searchParams.set("filesystem_id", filesystemId);
        const socket = new WebSocket(url.href);
        await opened(socket);
        return new StorageSocket(socket);
    }
    async exchangeToken(actions) {
        const subjectToken = await this.options.bearerToken();
        if (!subjectToken) {
            throw new Error("Missing OIDC access token");
        }
        const body = new URLSearchParams({
            grant_type: "urn:ietf:params:oauth:grant-type:token-exchange",
            subject_token: subjectToken,
            subject_token_type: "urn:ietf:params:oauth:token-type:access_token",
            audience: this.options.audience,
            client_id: this.options.clientId,
            authorization_details: JSON.stringify([
                { type: "storage", actions },
            ]),
        });
        const response = await (this.options.fetch ?? fetch)(new URL("/oauth2/token", parseUrl(this.options.baseUrl)), {
            method: "POST",
            headers: {
                "Content-Type": "application/x-www-form-urlencoded",
            },
            body,
        });
        if (!response.ok) {
            throw new Error(`Storage token exchange failed: ${response.status}`);
        }
        const result = (await response.json());
        if (!result.access_token) {
            throw new Error("Storage token exchange returned no access token");
        }
        return result.access_token;
    }
    request(path, token, init = {}) {
        return (this.options.fetch ?? fetch)(new URL(path, parseUrl(this.options.baseUrl)), {
            ...init,
            headers: { ...init.headers, Authorization: `Bearer ${token}` },
        });
    }
}
function parseUrl(value) {
    const url = new URL(value);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
        throw new Error("LIDP API URL must use HTTP or HTTPS");
    }
    return url;
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