export const OIDC_CLIENT_ERROR_CODES = [
    "NO_ACCESS_TOKEN",
    "NO_REFRESH_TOKEN",
    "NO_USERINFO_ENDPOINT",
    "HTTP_ERROR",
    "JSON_PARSE_ERROR",
    "NETWORK_TIMEOUT",
    "NETWORK_ERROR",
    "INVALID_USERINFO_RESPONSE",
    "MISSING_STATE",
    "INVALID_STATE",
] as const;

export type OidcClientErrorCode = (typeof OIDC_CLIENT_ERROR_CODES)[number];

export type OidcClientErrorDetails = {
    status?: number;
    statusText?: string;
    endpoint?: string;
    responseText?: string;
    cause?: unknown;
};

export class OidcClientError extends Error {
    readonly code: OidcClientErrorCode;
    readonly details?: OidcClientErrorDetails;

    constructor(
        code: OidcClientErrorCode,
        message: string,
        details?: OidcClientErrorDetails,
    ) {
        super(message);
        this.name = "OidcClientError";
        this.code = code;
        this.details = details;
    }
}
