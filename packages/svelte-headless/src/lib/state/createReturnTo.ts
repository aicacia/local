import { createStorage } from "./storage.svelte";

export type ReturnToOptions = {
    id: string;
    goto: (path: string) => Promise<void>;
    defaultPath?: string;
};

export function createReturnTo({ id, goto, defaultPath }: ReturnToOptions) {
    const returnTo = createStorage<string | null>(`return-to:${id}`, null);

    function setURL(url: URL) {
        returnTo.item = url.toString().substring(url.origin.length);
    }

    function setPath(path: string) {
        setURL(new URL(path, location.origin));
    }

    async function onReturn() {
        const returnToPath = returnTo.item ?? defaultPath;

        if (returnToPath) {
            returnTo.item = null;
            try {
                await goto(returnToPath);
            } catch (error) {
                returnTo.item = returnToPath;
                throw error;
            }
        }
    }

    return {
        setURL,
        setPath,
        onReturn,
    };
}
