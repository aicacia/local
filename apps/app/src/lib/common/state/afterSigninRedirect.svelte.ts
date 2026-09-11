import { createReturnTo } from "@aicacia/svelte-headless";
import { goto } from "$app/navigation";
import { resolve } from "$app/paths";

export const afterSigninRedirect = createReturnTo({
    id: "after-signin-redirect",
    goto,
    defaultPath: resolve("/"),
});
