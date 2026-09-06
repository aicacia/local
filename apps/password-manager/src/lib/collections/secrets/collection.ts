import { createCollection, localStorageCollectionOptions } from "@tanstack/db";
import { SecretSchema } from "./schema";

export const secretCollection = createCollection(
  localStorageCollectionOptions({
    id: 'secrets',
    schema: SecretSchema,
    getKey: (item) => item.id,
    storageKey: 'db:secrets'
  }),
);
