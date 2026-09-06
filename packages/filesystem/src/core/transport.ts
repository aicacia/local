import type { JsonRpcRequest } from "./jsonrpc.js";

export type PeerId = string | number;

export type SendRequest = JsonRpcRequest<"send", {
  to: PeerId;
}>;

export interface Message {
  type:
}

export interface Transport {
  send(message: Message): Promise<void>;
}
