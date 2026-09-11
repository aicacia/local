export function once<F extends (...args: any[]) => any>(fn: F): F {
    let called = false;
    let result: ReturnType<F>;

    return ((...args: Parameters<F>): ReturnType<F> => {
        if (!called) {
            called = true;
            result = fn(...args);
        }
        return result;
    }) as F;
}
