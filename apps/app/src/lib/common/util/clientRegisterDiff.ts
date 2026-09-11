import type { Client, ClientRegisterRequest } from "$lib/proto/mises.js";

const ARRAY_FIELDS = [
    "redirectUris",
    "grantTypes",
    "responseTypes",
    "contacts",
    "defaultAcrValues",
    "requestUris",
    "postLogoutRedirectUris",
] as const;
const STRING_FIELDS = [
    "name",
    "scope",
    "tokenEndpointAuthMethod",
    "applicationType",
    "clientUri",
    "logoUri",
    "policyUri",
    "tosUri",
    "jwksUri",
    "jwks",
    "sectorIdentifierUri",
    "subjectType",
    "idTokenSignedResponseAlg",
    "idTokenEncryptedResponseAlg",
    "idTokenEncryptedResponseEnc",
    "userinfoSignedResponseAlg",
    "userinfoEncryptedResponseAlg",
    "userinfoEncryptedResponseEnc",
    "requestObjectSigningAlg",
    "requestObjectEncryptionAlg",
    "requestObjectEncryptionEnc",
    "tokenEndpointAuthSigningAlg",
    "initiateLoginUri",
    "frontchannelLogoutUri",
    "backchannelLogoutUri",
] as const;
const BOOLEAN_FIELDS = [
    "requirePkce",
    "requireAuthTime",
    "frontchannelLogoutSessionRequired",
    "backchannelLogoutSessionRequired",
] as const;
const NUMBER_FIELDS = [
    "defaultMaxAge",
    "accessTokenExpiry",
    "refreshTokenExpiry",
] as const;

function normalizeOptionalString(
    value: string | undefined,
): string | undefined {
    const trimmed = value?.trim();
    return trimmed && trimmed.length > 0 ? trimmed : undefined;
}

function normalizeStringArray(value: string[] | undefined): string[] {
    if (!value) {
        return [];
    }

    const normalized = value
        .map((item) => item.trim())
        .filter((item) => item.length > 0)
        .sort();

    return [...new Set(normalized)];
}

function areStringArraysEqual(
    left: string[] | undefined,
    right: string[] | undefined,
): boolean {
    const normalizedLeft = normalizeStringArray(left);
    const normalizedRight = normalizeStringArray(right);

    if (normalizedLeft.length !== normalizedRight.length) {
        return false;
    }

    return normalizedLeft.every(
        (value, index) => value === normalizedRight[index],
    );
}

type DiffField =
    | (typeof ARRAY_FIELDS)[number]
    | (typeof STRING_FIELDS)[number]
    | (typeof BOOLEAN_FIELDS)[number]
    | (typeof NUMBER_FIELDS)[number];

export type ClientRegisterChangedFields = Partial<
    Pick<ClientRegisterRequest, DiffField>
>;

export function createChangedClientRegisterFields(
    requested: ClientRegisterRequest,
    existing: Client,
): ClientRegisterChangedFields | null {
    const diffRequest: ClientRegisterChangedFields = {};

    for (const field of ARRAY_FIELDS) {
        const requestedValue = requested[field];
        if (requestedValue === undefined) {
            continue;
        }
        const existingValue = existing[field];

        if (!areStringArraysEqual(requestedValue, existingValue)) {
            diffRequest[field] = requestedValue ?? [];
        }
    }

    for (const field of STRING_FIELDS) {
        const requestedValue = normalizeOptionalString(requested[field]);
        if (requestedValue === undefined) {
            continue;
        }
        const existingValue = normalizeOptionalString(existing[field]);

        if (requestedValue !== existingValue) {
            diffRequest[field] = requestedValue;
        }
    }

    for (const field of BOOLEAN_FIELDS) {
        const requestedValue = requested[field] ?? undefined;
        if (requestedValue === undefined) {
            continue;
        }
        const existingValue = existing[field] ?? undefined;

        if (requestedValue !== existingValue) {
            diffRequest[field] = requestedValue;
        }
    }

    for (const field of NUMBER_FIELDS) {
        const requestedValue = requested[field] ?? undefined;
        if (requestedValue === undefined) {
            continue;
        }
        const existingValue = existing[field] ?? undefined;

        if (requestedValue !== existingValue) {
            diffRequest[field] = requestedValue;
        }
    }

    return Object.keys(diffRequest).length > 0 ? diffRequest : null;
}
