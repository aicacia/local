import { json } from "@sveltejs/kit";
import { PUBLIC_CERT_FINGERPRINT } from "$env/static/public";

export const prerender = true;

export function GET() {
    return json([
        {
            relation: ["delegate_permission/common.handle_all_urls"],
            target: {
                namespace: "com.lidp",
                package_name: "com.lidp",
                sha256_cert_fingerprints: [PUBLIC_CERT_FINGERPRINT],
            },
        },
    ]);
}
