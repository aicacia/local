# ADR-001: IdP / OIDC / OAuth2 Service

## Status

Accepted

## Context

We need an identity provider that is compliant with OIDC/OAuth2 so that
standard relying parties (RPs), client libraries, and tooling can integrate
with it without custom protocol work. At the same time, we want user
identity to be rooted in public-key cryptography rather than a
password/credential store, consistent with the key-oriented design already
used elsewhere in the system (the DFS crate's content-addressed objects and
its always-Full Global Identity Namespace).

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
integrations require no special-casing beyond "how you prove control of
the key."

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
