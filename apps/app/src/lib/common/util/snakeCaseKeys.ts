type UncapitalizeSnakeCase<S extends string> =
    S extends `${infer P1}${infer P2}`
        ? P2 extends Uncapitalize<P2> // Check if P2 starts with a lowercase letter (implying P1 was an uppercase letter starting a new word)
            ? `${Uncapitalize<P1>}${UncapitalizeSnakeCase<P2>}`
            : `${Uncapitalize<P1>}_${UncapitalizeSnakeCase<P2>}`
        : S;

type SnakeCase<T> = {
    [K in keyof T as K extends string
        ? UncapitalizeSnakeCase<K>
        : never]: T[K] extends object ? SnakeCase<T[K]> : T[K];
};

export function snakeCaseKeys<T>(object: T): SnakeCase<T> {
    if (Array.isArray(object)) {
        return object.map(snakeCaseKeys) as SnakeCase<T>;
    } else if (object && typeof object === "object") {
        return Object.fromEntries(
            Object.entries(object).map(([key, value]) => [
                key.replace(/([A-Z])/g, "_$1").toLowerCase(),
                snakeCaseKeys(value),
            ]),
        ) as SnakeCase<T>;
    }
    return object as SnakeCase<T>;
}
