import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

export async function redirectToUrl(url: URL | string) {
    if (isTauri()) {
        await openUrl(url);
    } else {
        window.location.href = url.toString();
    }
}
