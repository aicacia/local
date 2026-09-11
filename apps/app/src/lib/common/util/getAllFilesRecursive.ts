import fs from "node:fs/promises";
import { join } from "node:path";

export async function getAllFilesRecursive(dirPath: string) {
    const filePaths: string[] = [];
    const entries = await fs.readdir(dirPath, { withFileTypes: true });

    for (const entry of entries) {
        const fullPath = join(dirPath, entry.name);

        if (entry.isDirectory()) {
            filePaths.push(...(await getAllFilesRecursive(fullPath)));
        } else if (entry.isFile()) {
            filePaths.push(fullPath);
        }
    }

    return filePaths;
}
