import { handleNativeCallbackRequestUrl } from "@aicacia/native-fetch";
import { goto } from "$app/navigation";
import { loadLocalhostBaseUrl } from "../state/localhostBaseUrl.svelte";
import { redirectToUrl } from "./redirectToUrl";

export async function handleDeepLink(urlStrings: string[]): Promise<void> {
  const [urlString] = urlStrings;
  if (!urlString) {
    return;
  }

  let url: URL;
  try {
    url = new URL(urlString);
  } catch {
    return;
  }

  if (url.protocol === "lidp:" && url.hostname === "pair") {
    const invitation = url.searchParams.get("invitation");
    const secret = url.searchParams.get("secret");
    if (
      !invitation ||
      !secret ||
      invitation.length > 256 ||
      secret.length > 4096
    ) {
      return;
    }
    await goto(
      `/devices?invitation=${encodeURIComponent(invitation)}&secret=${encodeURIComponent(secret)}`,
    );
    return;
  }

  if (!url.searchParams.has("native")) {
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      return;
    }
    const response = await fetch(url);

    if (response.status >= 300 && response.status < 400) {
      const location = response.headers.get("location");

      if (location) {
        const redirectUrl = new URL(location);

        if (redirectUrl.origin === url.origin) {
          await goto(
            redirectUrl.pathname + redirectUrl.search + redirectUrl.hash,
          );
        }
      }
    }

    return;
  }

  const callbackUrl = await handleNativeCallbackRequestUrl(
    url,
    async (request) => {
      const requestUrl = new URL(request.url);

      if (requestUrl.pathname === "/server-base-url") {
        const baseUrl = await loadLocalhostBaseUrl();
        return new Response(
          JSON.stringify({
            baseUrl: baseUrl ?? "",
          }),
          {
            headers: {
              "content-type": "application/json;charset=UTF-8",
            },
          },
        );
      }

      return fetch(requestUrl, {
        method: request.method,
        headers: request.headers,
        body:
          request.method === "GET" || request.method === "HEAD"
            ? undefined
            : await request.arrayBuffer(),
      });
    },
  );

  await redirectToUrl(callbackUrl);
}
