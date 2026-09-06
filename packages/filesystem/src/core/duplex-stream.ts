export interface DuplexStream {
  readable: ReadableStream<Uint8Array>;
  writable: WritableStream<Uint8Array>;

  close(): Promise<void>;
}

export function createDuplexStream(
  readable: ReadableStream<Uint8Array>,
  writable: WritableStream<Uint8Array>,
): DuplexStream {
  return {
    readable,
    writable,
    async close() {
      await writable.close();
      await readable.cancel();
    },
  };
}
