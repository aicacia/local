import { redirect } from "@sveltejs/kit";
import type { PageLoad } from "./$types";
import { afterSigninRedirect } from "$lib/common/state/afterSigninRedirect.svelte";
import { notifications } from "$lib/common/state/notifications.svelte";
import { getOidcClient } from "$lib/common/state/oidc.svelte";

export const load: PageLoad = async (event) => {
    await event.parent();

    try {
        const oidcClient = getOidcClient();
        await oidcClient.handleSigninCallback(event.url);
    } catch (e) {
        if (e instanceof Error) {
            notifications.add(e.message);
        }
        redirect(302, "/signin");
    }
    await afterSigninRedirect.onReturn();
};
