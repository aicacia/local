import * as v from "valibot";
import type { StorageResponse, StorageSocket } from "$lib/common/storageSocket";
import { SecretSchema, type Secret } from "$lib/models/secret";

const DIRECTORY = "secrets";
const EXTENSION = ".json";
const encoder = new TextEncoder();
const decoder = new TextDecoder();

export class SecretStorage {
    constructor(private readonly socket: StorageSocket) {}

    async list(): Promise<Secret[]> {
        const response = await this.socket.request({
            type: "list",
            path: DIRECTORY,
        });
        if (response.type !== "listed") {
            throw storageError(response);
        }

        return Promise.all(
            response.entries
                .filter((entry) => entry.name.endsWith(EXTENSION))
                .map((entry) => this.read(entry.name)),
        );
    }

    async save(secret: Secret): Promise<void> {
        const content = encoder.encode(
            JSON.stringify(v.parse(SecretSchema, secret)),
        );
        const response = await this.socket.request({
            type: "write",
            path: path(secret.id),
            content: [...content],
        });
        if (response.type !== "written") {
            throw storageError(response);
        }
    }

    async delete(id: string): Promise<void> {
        const response = await this.socket.request({
            type: "delete",
            path: path(id),
        });
        if (response.type !== "deleted") {
            throw storageError(response);
        }
    }

    private async read(name: string): Promise<Secret> {
        const response = await this.socket.request({
            type: "read",
            path: `${DIRECTORY}/${name}`,
        });
        if (response.type !== "read") {
            throw storageError(response);
        }
        return v.parse(
            SecretSchema,
            JSON.parse(decoder.decode(new Uint8Array(response.content))),
        );
    }
}

function path(id: string): string {
    return `${DIRECTORY}/${id}${EXTENSION}`;
}

function storageError(response: StorageResponse): Error {
    return new Error(
        response.type === "error"
            ? `Storage request failed: ${response.code}`
            : "Unexpected storage response",
    );
}
