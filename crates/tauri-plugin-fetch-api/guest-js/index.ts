import { Channel, invoke } from "@tauri-apps/api/core";

// 1. Align types with the backend StreamFrame enum structure
type StreamFrame =
  | {
      type: "ResponseHead";
      payload: { status: number; headers: [string, string][] };
    }
  | { type: "Data"; payload: number[] }
  | { type: "Error"; payload: string }
  | { type: "Complete" };

const TEXT_DECODER = new TextDecoder();
const EMPTY_CHUNK = new Uint8Array(0);

interface InitStreamResponse {
  stream_id: string;
}

export async function fetch(
  input: RequestInfo | URL,
  init?: RequestInit,
): Promise<Response> {
  try {
    let path: string;
    let method = init?.method;
    let headers = init?.headers;
    let body = init?.body;
    let signal = init?.signal;

    // Unify parameter extraction depending on whether a Request object or string/URL layout was supplied
    if (typeof input === "object" && "url" in input) {
      const req = input as Request;
      path = req.url;
      method = method || req.method;
      headers = headers || req.headers;
      body = body || req.body;
      signal = signal || req.signal;
    } else {
      path = input.toString();
    }

    const requestMethod = method || "GET";

    if (signal?.aborted) {
      console.warn(
        `Abort error triggered early before initializing stream context for: "${path}"`,
      );
      throw new DOMException("The user aborted a request.", "AbortError");
    }

    // Set up a channel expecting structured StreamFrames instead of raw binary chunks
    const responseChannel = new Channel<StreamFrame>();
    let registeredStreamId: string | null = null;
    let streamController: ReadableStreamDefaultController<Uint8Array> | null =
      null;
    let isResponseResolved = false;

    // Create the stream body container early, but control it asynchronously via channel messages
    const browserReadableStream = new ReadableStream<Uint8Array>({
      start(controller) {
        streamController = controller;
      },
    });

    // Safely normalize headers supporting standard Headers objects, records, or multi-dimensional entries arrays
    const normalizedHeaders = new Headers(headers || {});

    if (isFormData(body)) {
      normalizedHeaders.set("Content-Type", "multipart/form-data");
    } else if (isURLSearchParams(body)) {
      normalizedHeaders.set(
        "Content-Type",
        "application/x-www-form-urlencoded",
      );
    }

    const payload = {
      method: requestMethod,
      uri: path,
      headers: Array.from(normalizedHeaders.entries()),
      response_channel: responseChannel,
    };

    // The Promise executor function is now strictly synchronous to prevent unhandled rejection leaks
    return new Promise<Response>((resolve, reject) => {
      // Wire up the multiplexed message listener before invoking the backend command
      responseChannel.onmessage = (frame: StreamFrame) => {
        switch (frame.type) {
          case "ResponseHead":
            isResponseResolved = true;
            // Unblock the standard fetch consumer loop by returning the Response shell
            resolve(
              new Response(browserReadableStream, {
                status: frame.payload.status,
                headers: new Headers(frame.payload.headers),
              }),
            );
            break;

          case "Data":
            if (streamController) {
              // Convert numerical array payload bytes back into target buffer elements
              const bytes = new Uint8Array(frame.payload);
              streamController.enqueue(bytes);
            } else {
              console.error(
                `[Channel Message] Missed target runtime: Stream controller uninitialized during data packet processing`,
              );
            }
            break;

          case "Error": {
            const errorException = new DOMException(
              frame.payload,
              "NetworkError",
            );
            if (!isResponseResolved) {
              reject(errorException);
            } else if (streamController) {
              streamController.error(errorException);
            }
            break;
          }

          case "Complete":
            if (streamController) {
              try {
                streamController.close();
              } catch (err) {
                console.debug(
                  `[Channel Message] Exception suppressed during stream closure handling:`,
                  err,
                );
                // The controller might already be in an error state or closed
              }
            }
            break;
        }
      };

      if (signal) {
        signal.addEventListener("abort", async () => {
          if (registeredStreamId) {
            try {
              await invoke("plugin:fetch-api|cancel_plugin_stream", {
                streamId: registeredStreamId,
              });
            } catch (e) {
              console.error(
                "[Abort Trigger] Failed to notify stream cancel route:",
                e,
              );
            }
          }

          const abortError = new DOMException(
            "The user aborted a request.",
            "AbortError",
          );
          if (!isResponseResolved) {
            reject(abortError);
          } else if (streamController) {
            streamController.error(abortError);
          }
        });
      }

      // Isolate the asynchronous orchestration pipeline into an immediate invoked async function expression
      (async () => {
        try {
          // 2. Invoke the initialization command (returns an ArrayBuffer of the response bytes)
          const rawHeadBytes = await invoke<ArrayBuffer>(
            "plugin:fetch-api|init_plugin_stream",
            { request: payload },
          );

          // Decode the binary response buffer into a string and extract the stream identification token
          const headString = TEXT_DECODER.decode(rawHeadBytes);

          const initHead: InitStreamResponse = JSON.parse(headString);
          registeredStreamId = initHead.stream_id;

          if (signal?.aborted) {
            await invoke("plugin:fetch-api|cancel_plugin_stream", {
              streamId: registeredStreamId,
            });
            reject(
              new DOMException("The user aborted a request.", "AbortError"),
            );
            return;
          }

          // 3. Immediately kick off request payload streaming concurrently.
          if (body) {
            const uploadReader = ensureReadableStream(body).getReader();

            try {
              let iterationIndex = 0;
              while (true) {
                const { value, done } = await uploadReader.read();
                const chunkData = value ?? EMPTY_CHUNK;

                // Map parameters back to standard camelCase arguments expected by the Tauri command macro
                await invoke("plugin:fetch-api|upload_plugin_chunk", {
                  streamId: registeredStreamId,
                  chunk: chunkData,
                  isEof: done,
                });

                if (done || signal?.aborted) {
                  break;
                }
                iterationIndex++;
              }
            } catch (uploadError) {
              console.error(
                "[Async Orchestrator] [Upload Loop] Error broken inside core processing streaming execution track:",
                uploadError,
              );
              await invoke("plugin:fetch-api|upload_plugin_chunk", {
                streamId: registeredStreamId,
                chunk: [],
                isEof: true,
              });
            } finally {
              uploadReader.releaseLock();
            }
          } else {
            // Automatically close empty request pipes
            await invoke("plugin:fetch-api|upload_plugin_chunk", {
              streamId: registeredStreamId,
              chunk: [],
              isEof: true,
            });
          }
        } catch (initError) {
          console.error(
            `[Async Orchestrator] Initialization lifecycle failure block captured:`,
            initError,
          );
          if (!isResponseResolved) {
            reject(initError);
          }
        }
      })();
    });
  } catch (error) {
    console.error("[tauri-fetch] Unhandled exception in fetch wrapper:", error);
    throw error;
  }
}

function ensureReadableStream(body: BodyInit): ReadableStream<Uint8Array> {
  if (body instanceof ReadableStream) {
    return body as ReadableStream<Uint8Array>;
  }
  const stream = new Response(body).body;
  if (!stream) {
    throw new DOMException(
      "failed to generate a readable stream from the provided body.",
      "InvalidStateError",
    );
  }
  return stream;
}

function isFormData(body?: any): body is FormData {
  if (!body) {
    return false;
  }
  return body instanceof FormData;
}

function isURLSearchParams(body: any): body is URLSearchParams {
  if (!body) {
    return false;
  }

  return (
    body instanceof URLSearchParams ||
    (typeof body === "object" &&
      "append" in body &&
      typeof body.append === "function")
  );
}
