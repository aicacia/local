import { handleNativeCallbackRequestUrl } from "@aicacia/native-fetch";
import { fetch as tauriFetch } from "tauri-plugin-fetch-api";

import { getStorageBridgeConfig } from "../state/storageClient.svelte";
import { redirectToUrl } from "./redirectToUrl";
import { goto } from "$app/navigation";

export async function handleDeepLink(urlStrings: string[]): Promise<void> {
  const [urlString] = urlStrings;
  if (!urlString) {
    return;
  }

  const url = new URL(urlString);

  if (!url.searchParams.has("native")) {
    const response = await tauriFetch(url);

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

      if (requestUrl.pathname === "/bridge-url") {
        const bridgeUrlConfig = await getStorageBridgeConfig();
        return new Response(
          JSON.stringify({
            bridgeUrl: bridgeUrlConfig?.storageBridgeUrl ?? "",
          }),
          {
            headers: {
              "content-type": "application/json;charset=UTF-8",
            },
          },
        );
      }

      return tauriFetch(requestUrl, {
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
