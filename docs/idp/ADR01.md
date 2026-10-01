# ADR-001: IdP / OIDC / OAuth2 Service

## Status

Accepted

## Context

We need an identity provider that is compliant with OIDC/OAuth2 so that
standard relying parties (RPs), client libraries, and tooling can integrate
with it without custom protocol work. At the same time, we want user
identity is rooted in public-key cryptography rather than a password-only
credential store, consistent with the key-oriented design used elsewhere in
the system.

The central design tension is that OIDC expects a subject identifier
(`sub`) to be stable for a given client over time, while we want user keys
to be rotatable and, ideally, unlinkable across relying parties for
privacy.

## Decision

**Key hierarchy: BIP32 per user.**
Each user has one master key. All per-relying-party identity keys are
derived from that master via BIP32-style hierarchical deterministic
derivation. The master key itself never signs anything and is not exposed
to relying parties; only derived subkeys are used for actual
authentication.

**Protocol model: classic OIDC, not self-issued (SIOP).**
The IdP holds its own signing keypair and issues/signs ID tokens and access
tokens itself, after the user has proved control of the relevant derived
key via a signature challenge. This keeps us compatible with standard
OIDC/OAuth2 client libraries and relying-party tooling, rather than
requiring RPs to implement self-issued-token validation.

**Subject identifier: user-selected, per-client, pairwise-stable.**
At the point of approving an OAuth2 authorization request, the user selects
(on the client) which master-derived subkey to present as their identity to
that relying party. The IdP records which derived key was chosen for that
specific client and reuses the same key/derivation path for that
client on all future approvals. This gives:

- A stable `sub` per relying party (satisfying standard OIDC client
  assumptions), and
- Unlinkability across relying parties, since a user may present a
  different derived key to each one, as a natural side effect of BIP32
  per-RP derivation rather than a bespoke mechanism.

**Authentication flow: standard OAuth2 conventions.**
The login/authorization challenge flow (redirect-based `/authorize`,
signature challenge in place of a password, `/token` exchange, PKCE for
public clients) follows standard OAuth2 shapes so existing client
integrations require no special-casing beyond "how you prove control of the
key."

**Initial lifecycle: explicit setup before the router is ready.**
A newly opened local database is in `needs setup` state. Startup must not
invent credentials, run baseline bootstrap from ambient configuration, or
start normal DB-backed routes before setup completes. The setup flow offers
exactly two choices:

1. **Create a new system.** The user supplies the device name and admin
   username/password. The application initializes the schema, writes the
   complete baseline and the first approved device through the DB repositories,
   then constructs and starts the router on the next application start.
2. **Join an existing system.** The user signs in to the existing IdP and
   authorizes this device through its authenticated setup API. Device-to-control-
   plane authorization/registration may use HTTPS. The control-plane database is
   then synchronized directly between devices using `ofdb` sync over an Iroh
   bidirectional stream. Device-to-device HTTP(S) synchronization is not used.
   Bootstrap stream admission must be explicitly authorized and bound to the
   registered Iroh endpoint identity; application-resource selection does not
   authorize control-plane bootstrap. Only after the local DB is durably complete
   and conflict-free does the joining application construct and start its normal
   router on the next application start. Setup completion does not hot-swap a
   running router.

**Storage authorization is separate from setup synchronization.** A signed-in
user receives a storage-audience OAuth access token for Storage API requests.
That token does not authorize mesh sync. Device enrollment and control-plane
DB replication use setup authorization and the Iroh stream, not a filesystem
sync session. This supersedes the earlier checkpoint/envelope and HTTP
`/setup/sync` proposal above.

Setup credentials are entered by the user during setup; they are not default
configuration values. A local DB that is empty, incomplete, conflicted, or
not safely authorized remains in setup or denial state rather than starting
partially initialized services.

**Standard OIDC surface required regardless of the above:**

- `/.well-known/openid-configuration` discovery document
- JWKS endpoint (IdP's own signing keys, not user keys)
- `/authorize`, `/token`, `/userinfo`
- Refresh token support

## Consequences

- Relying parties get a normal OIDC integration experience; no custom
  token-validation logic required on their side.
- User key rotation is possible without breaking relying-party
  recognition of the user, because the identity primitive is the
  BIP32 derivation path (remembered by the IdP per-client), not the raw
  key itself.
- The IdP becomes the sole holder of the "which derived key belongs to
  which client" mapping; its availability and integrity are on the
  critical path for login. This mapping is sensitive and needs the same
  care as a credential store even though no passwords are involved.
- Per-RP unlinkability depends on users (or their client software)
  consistently choosing distinct subkeys per RP; if a client always
  reuses one subkey across all RPs, this property degrades. Client
  UX/defaults should nudge toward per-RP keys.
- Because the IdP signs tokens itself, key compromise of the IdP's own
  signing key (distinct from any user key) is a full trust-root
  compromise and needs standard KMS/HSM-grade protection.

## Alternatives Considered

- **Self-issued OP (SIOP)**: rejected because it requires bespoke
  validation logic on every relying party and doesn't benefit from
  existing OIDC tooling.
- **`sub` = raw public key, no derivation**: rejected because it makes
  key rotation equivalent to identity loss from the RP's point of view.
