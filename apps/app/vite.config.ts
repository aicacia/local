import { paraglideVitePlugin } from "@inlang/paraglide-js";
import { sveltekit } from "@sveltejs/kit/vite";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";
import devtoolsJson from "vite-plugin-devtools-json";

function parseInteger(value?: string, defaultValue?: number): number {
    try {
        return Number.parseInt(value, 10);
    } catch {
        return defaultValue;
    }
}

export default defineConfig({
    plugins: [
        tailwindcss(),
        sveltekit(),
        devtoolsJson(),
        paraglideVitePlugin({
            project: "./project.inlang",
            outdir: "./src/lib/paraglide",
            strategy: ["localStorage", "baseLocale"],
        }),
    ],
    server: {
        port: parseInteger(process.env.PORT, 5173),
        strictPort: true,
    },
    preview: {
        port: parseInteger(process.env.PORT, 5173),
        strictPort: true,
    },
});
