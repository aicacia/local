import { defineConfig } from "rolldown";

export default defineConfig({
    input: "src/index.ts",
    output: {
        file: "browser/index.js",
    },
    plugins: [
        {
            name: "esm-import-to-url",
            resolveId(source, importer) {
                const urlMap = {
                    tslib: "https://unpkg.com/tslib@2/tslib.es6.js",
                };

                if (urlMap[source]) {
                    return { id: urlMap[source], external: true };
                }

                return null;
            },
        },
    ],
});
