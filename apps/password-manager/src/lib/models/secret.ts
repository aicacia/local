import * as v from "valibot";

export const UriMatchType = {
    Domain: "domain",
    Host: "host",
    StartsWith: "startsWith",
    Exact: "exact",
    RegularExpression: "regex",
    Never: "never",
} as const;

export type UriMatchType =
    (typeof UriMatchType)[keyof typeof UriMatchType];

export const UriMatchTypeSchema = v.picklist([
    UriMatchType.Domain,
    UriMatchType.Host,
    UriMatchType.StartsWith,
    UriMatchType.Exact,
    UriMatchType.RegularExpression,
    UriMatchType.Never,
]);

export const FieldType = {
    Text: "text",
    Hidden: "hidden",
    Boolean: "boolean",
} as const;

export type FieldType = (typeof FieldType)[keyof typeof FieldType];

export const FieldTypeSchema = v.picklist([
    FieldType.Text,
    FieldType.Hidden,
    FieldType.Boolean,
]);

export const UriSchema = v.object({
    uri: v.pipe(v.string(), v.minLength(1)),
    match: v.optional(UriMatchTypeSchema, UriMatchType.Domain),
});

export const CustomFieldSchema = v.object({
    name: v.pipe(v.string(), v.minLength(1)),
    value: v.string(),
    type: FieldTypeSchema,
});

export const SecretHistoryEntrySchema = v.object({
    secret: v.string(),
    changedAt: v.pipe(v.string(), v.isoTimestamp()),
});

export const SecretSchema = v.object({
    id: v.pipe(v.string(), v.uuid()),
    name: v.pipe(v.string(), v.minLength(1)),
    secret: v.pipe(v.string(), v.minLength(1)),
    uris: v.optional(v.array(UriSchema), []),
    notes: v.optional(v.string()),
    favorite: v.optional(v.boolean(), false),
    fields: v.optional(v.array(CustomFieldSchema), []),
    history: v.optional(v.array(SecretHistoryEntrySchema), []),
    createdAt: v.pipe(v.string(), v.isoTimestamp()),
    updatedAt: v.pipe(v.string(), v.isoTimestamp()),
    deletedAt: v.optional(v.pipe(v.string(), v.isoTimestamp())),
});

export type Secret = v.InferOutput<typeof SecretSchema>;
