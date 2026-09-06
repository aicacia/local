export type StorageRequest = {
    type: "read";
    path: string;
} | {
    type: "write";
    path: string;
    content: number[];
} | {
    type: "append";
    path: string;
    content: number[];
} | {
    type: "delete";
    path: string;
} | {
    type: "entry";
    path: string;
} | {
    type: "list";
    path: string;
};
export type StorageEntry = {
    name: string;
    hash: string;
    size: number;
    local: boolean;
};
export type StorageResponse = {
    type: "authenticated";
} | {
    type: "read";
    content: number[];
} | {
    type: "written";
    entry: StorageEntry;
} | {
    type: "appended";
    entry: StorageEntry;
} | {
    type: "deleted";
} | {
    type: "entry";
    entry: StorageEntry;
} | {
    type: "listed";
    entries: StorageEntry[];
} | {
    type: "error";
    code: "invalidRequest" | "operationFailed";
};
type FetchFunction = (input: URL | RequestInfo, init?: RequestInit) => Promise<Response>;
export type StorageClientOptions = {
    baseUrl: URL | string;
    bearerToken: () => string | Promise<string>;
    fetch?: FetchFunction;
};
export declare class StorageSocket {
    #private;
    readonly socket: WebSocket;
    constructor(socket: WebSocket);
    request(request: StorageRequest): Promise<StorageResponse>;
    close(): void;
}
export declare class StorageClient {
    private readonly options;
    constructor(options: StorageClientOptions);
    openSocket(): Promise<StorageSocket>;
}
export {};
//# sourceMappingURL=index.d.ts.map