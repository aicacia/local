type CamelizeSnakeCase<S extends string> = S extends `${infer P1}_${infer P2}`
    ? `${P1}${CamelizeSnakeCase<Capitalize<P2>>}`
    : S;

type CamelCase<T> = {
    [K in keyof T as K extends string
        ? CamelizeSnakeCase<K>
        : never]: T[K] extends object ? CamelCase<T[K]> : T[K];
};

export function camelizeKeys<T>(obj: T): CamelCase<T> {
    if (Array.isArray(obj)) {
        return obj.map(camelizeKeys) as CamelCase<T>;
    } else if (obj && typeof obj === "object") {
        return Object.fromEntries(
            Object.entries(obj).map(([key, value]) => [
                key.replace(/_([a-z])/g, (_, c) => c.toUpperCase()),
                camelizeKeys(value),
            ]),
        ) as CamelCase<T>;
    }
    return obj as CamelCase<T>;
}
