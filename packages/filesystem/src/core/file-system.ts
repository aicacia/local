import type { FileSystemStorage } from "./FileSystemStorage.js";

export interface FileSystemOptions {
  storage: FileSystemStorage;
}

export class FileSystem {
  private readonly storage: FileSystemStorage;

  constructor({ storage }: FileSystemOptions) {
    this.storage = storage;
  }
}
