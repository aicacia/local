import { text } from "@sveltejs/kit";
import { resolve } from "$app/paths";

export const prerender = true;

export async function GET() {
    return text(`User-agent: *
Allow: /

Sitemap: ${resolve("/sitemap.xml")}`);
}
