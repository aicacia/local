import { isTauri } from "@tauri-apps/api/core";
import { redirect } from "@sveltejs/kit";
import { resolve } from "$app/paths";
import { afterSigninRedirect } from "$lib/common/state/afterSigninRedirect.svelte";
import {
  ensureTauriLidpApiUrl,
  getLidpApiUrl,
} from "$lib/common/state/lidpClient.svelte";
import { getOidcClient } from "$lib/common/state/oidc.svelte";
import type { LayoutLoad } from "./$types";

export const load: LayoutLoad = async (event) => {
  await event.parent();

  if (isTauri()) {
    await ensureTauriLidpApiUrl();
  }

  const oidcClient = getOidcClient();
  const token = oidcClient.getStoredTokenResponse();
  if (
    !token?.access_token ||
    (isTauri() && token.iss && token.iss !== getLidpApiUrl())
  ) {
    oidcClient.clearStoredTokenResponse();
    afterSigninRedirect.setURL(event.url);
    redirect(302, resolve("/signin"));
  }

  try {
    const currentUserInfo = await oidcClient.getUserInfo();

    if (currentUserInfo) {
      return {
        userInfo: currentUserInfo,
      };
    }
  } catch {
    oidcClient.clearStoredTokenResponse();
    afterSigninRedirect.setURL(event.url);
    redirect(302, resolve("/signin"));
  }
};
