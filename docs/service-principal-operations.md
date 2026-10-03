# Service principal operations

This guide covers the OAuth clients used by Management and Storage. Each deployment environment must have distinct confidential client registrations and secrets. Do not reuse a service principal across environments or use a user's credentials for background service calls.

## Required registrations

The public `POST /oauth2/register` and `PUT /oauth2/register/{client_id}` routes reject `client_credentials`; do not use dynamic user registration to create service principals. Their user bearer check does not prove ownership of a client or grant IdP administration rights. Do not use these routes to create, update, or delete service clients until owner authorization exists. First-client provisioning must be a local IdP operator CLI command that writes directly to IdP-owned state before dependent services start. Do not reuse `/setup/new`, which is an existing unauthenticated installation-setup route, or add another HTTP setup/auth path. The local command is `idp-server --config <path> provision-service-client --application-uri <uri> --client-name <name> --audience <audience> --scope <scope> --credentials-file <protected-path>`. Repeat `--audience` and `--scope` for each allowed value. Stop the IdP process before running the command. The CLI help repeats this requirement, but the command does not detect or enforce that IdP is stopped. The command requires an existing application and database, creates a confidential `client_credentials` client, and writes `client_id` and `client_secret` to a new JSON file. On Unix, it creates that file with mode `0600` and refuses to overwrite an existing file. It does not print or log the secret. Repository-backed tests verify client creation, file permissions, exclusive creation, and rollback after injected failures, including cleanup of partially written key material. These tests do not replace live-listener service authorization checks. On non-Unix systems, it returns an unsupported-operation error because owner-only file permissions are not implemented.

| Caller     | Owner API  | Allowed audience                       | Required scopes                                               |
| ---------- | ---------- | -------------------------------------- | ------------------------------------------------------------- |
| Management | IdP        | Configured IdP service audience        | `idp.token.validate`, `idp.device.lookup`                     |
| Storage    | IdP        | Configured IdP service audience        | `idp.token.validate`, `idp.device.lookup`, `idp.device.list`  |
| Storage    | Management | Configured Management service audience | `management.replication.read`, `management.replication.admit` |

For IdP introspection, each IdP client registration must also allow the audiences of the tokens that the caller needs to inspect. Keep this list limited to the actual user-token audiences for that deployment. A token's requested scope and audience must both be allowed at issuance, and the receiving API checks its own required scope and audience.

Service registrations must be confidential clients with the `client_credentials` grant. Client credentials only obtain access tokens; normal owner APIs still require those bearer tokens. Do not grant user-only actions, device enrollment/approval, or identity administration to service clients.

## Secret delivery

- Load each client ID and secret from the deployment's secret manager or a protected environment/secret file. Do not commit populated config or environment files.
- Restrict secret access to the service process and operators who provision or rotate it.
- Use TLS for remote service URLs. Plain HTTP is only for loopback development.
- Do not log token request forms, OAuth client registrations, token responses, bearer tokens, or secret values. Redact these fields from request tracing and error reports.

## Rotate a service client

Use a second client registration for rotation. Do not update a live client's secret in place while some instances still use the old value.

1. Create a new confidential client under the same service owner. Grant only the audience and scopes in the table above, plus the minimal inspected-token audiences needed by that caller.
2. Store the new secret in the deployment secret manager. Do not send it through chat, tickets, or logs.
3. Update the caller's secret reference and client ID. Roll out instances without removing the old registration.
4. Confirm the caller can obtain a token and complete its owner API operation. Check that wrong audiences and ungranted scopes remain denied.
5. Remove the old client registration through an authorized IdP owner operation. IdP can no longer resolve the client principal for its signing key, so clients must fail closed when they next request or validate its token.
6. Remove the old secret from the deployment secret manager and record the rotation date and registration IDs, never the secret.

If rotation fails, restore the prior secret reference only while the old client remains active. Do not loosen scopes or audiences to make a failed rollout pass.

## Revoke a compromised client

1. Delete or deactivate the client registration at IdP using an authorized owner operation.
2. Remove the secret from all deployment secret stores and stop or reconfigure every caller using it.
3. Verify that new token issuance fails and that an already-issued token is rejected by authoritative IdP validation.
4. Create a replacement registration with a new ID and secret only after the compromise is contained.

There is no offline authorization guarantee. If IdP or Management is unavailable, dependent operations fail closed. Do not add a token-validation cache to hide an authority outage.

## Current implementation limits

The local `provision-service-client` command and repository-level tests exist. A temporary fixture provisions distinct Management→IdP, Storage→IdP, and Storage→Management clients, then checks token issuance and audience/scope rejection. A live Management listener test rejects an admission-only token for a read operation after introspection through an IdP HTTP stub. Actual deployment principals and the live IdP listener are not yet tested end to end. HTTP create/update/delete still lack a verifiable IdP owner/admin permission. Do not bypass that gap by enabling `client_credentials` in dynamic registration or adding an unauthenticated/internal route. The unified host and end-to-end provisioning workflow are also incomplete. Confirm actual audience identifiers from deployment configuration before provisioning; do not assume the internal HTTP base URL is the token issuer or audience.
