import { json } from "@sveltejs/kit";
import { resolve } from "$app/paths";
import icon256x256 from "$lib/assets/icon256x256.png";

export const prerender = true;

export async function GET() {
    return json({
        name: "Local Identity Provider",
        short_name: "LIdP",
        description: "Local Identity Provider",
        version: "1.0",
        manifest_version: 3,
        icons: [
            {
                src: icon256x256,
                sizes: "256x256",
                type: "image/png",
            },
        ],
        id: `${resolve("/")}?source=pwa`,
        start_url: `${resolve("/")}?source=pwa`,
        scope: resolve("/"),
        display: "standalone",
        background_color: "white",
        theme_color: "white",
    });
}
