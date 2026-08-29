import type { AuthorizationRequest } from "@aicacia/lidp-client";
import { lidpApi } from "$lib/common/state/lidpClient.svelte";
import { redirectToUrl } from "$lib/common/util/redirectToUrl";

export async function rejectAuthorizationRequest(
  authorizationRequest: Pick<
    AuthorizationRequest,
    "redirectUri" | "state" | "nonce"
  >,
  error: string,
  errorDescription: string,
) {
  const url = new URL(authorizationRequest.redirectUri!);
  if (authorizationRequest.state) {
    url.searchParams.set("state", authorizationRequest.state);
  }
  if (authorizationRequest.nonce) {
    url.searchParams.set("nonce", authorizationRequest.nonce);
  }
  url.searchParams.set("error", error);
  url.searchParams.set("error_description", errorDescription);
  await redirectToUrl(url);
}

export async function resolveAuthorizationRequest(
  authorizationRequest: AuthorizationRequest,
) {
  const authorizeResponse = await lidpApi.authorizeJson({
    authorizationRequest,
  });

  if ("error" in authorizeResponse) {
    throw new Error(
      authorizeResponse.errorDescription ?? authorizeResponse.error,
    );
  }

  if (!authorizeResponse.code) {
    throw new Error("authorization code response did not include code");
  }

  const url = new URL(authorizationRequest.redirectUri!);
  url.searchParams.set("code", authorizeResponse.code);
  if (authorizeResponse.iss) {
    url.searchParams.set("iss", authorizeResponse.iss);
  }
  if (authorizeResponse.state ?? authorizationRequest.state) {
    url.searchParams.set(
      "state",
      authorizeResponse.state ?? authorizationRequest.state!,
    );
  }
  if (authorizationRequest.nonce) {
    url.searchParams.set("nonce", authorizationRequest.nonce);
  }

  return await redirectToUrl(url);
}
