export type RGBA = [r: number, g: number, b: number, a: number];

export async function getAverageLuminance(
    imageElement: HTMLImageElement,
    size = 8,
    canvas = document.createElement("canvas"),
): Promise<number> {
    await waitForLoad(imageElement);

    canvas.width = size;
    canvas.height = size;

    const ctx = canvas.getContext("2d");
    if (!ctx) {
        throw new Error("Canvas context not available");
    }

    ctx.drawImage(imageElement, 0, 0, size, size);

    const { data } = ctx.getImageData(0, 0, size, size);

    let totalLuminance = 0;
    const pixelCount = size * size;

    for (let i = 0; i < data.length; i += 4) {
        const r = data[i];
        const g = data[i + 1];
        const b = data[i + 2];
        const a = data[i + 3] / 255;

        const blendedR = 255 * (1 - a) + r * a;
        const blendedG = 255 * (1 - a) + g * a;
        const blendedB = 255 * (1 - a) + b * a;

        const luminance =
            0.2126 * blendedR + 0.7152 * blendedG + 0.0722 * blendedB;
        totalLuminance += luminance;
    }

    return totalLuminance / pixelCount;
}

export function waitForLoad(imageElement: HTMLImageElement) {
    return new Promise<void>((resolve, reject) => {
        function onLoad() {
            clear();
            resolve();
        }
        function onError(e: ErrorEvent) {
            clear();
            reject(e);
        }
        function clear() {
            imageElement.removeEventListener("load", onLoad);
            imageElement.removeEventListener("error", onError);
        }

        imageElement.addEventListener("load", onLoad);
        imageElement.addEventListener("error", onError);
    });
}
