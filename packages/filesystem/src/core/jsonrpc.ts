export type JsonValue =
  string | number | boolean | null | { [key: string]: JsonValue } | JsonValue[];

export type JsonRpcVersion = "2.0";

export type JsonRpcId = string | number;

export interface JsonRpcRequest<M extends string = string, P = unknown> {
  jsonrpc: JsonRpcVersion;
  method: M;
  params?: P;
  id: JsonRpcId;
}

export interface JsonRpcNotification<M extends string = string, P = unknown> {
  jsonrpc: JsonRpcVersion;
  method: M;
  params?: P;
}

export enum JsonRpcErrorCode {
  ParseError = -32700,
  InvalidRequest = -32600,
  MethodNotFound = -32601,
  InvalidParams = -32602,
  InternalError = -32603,
}

export interface JsonRpcErrorStructure<
  D = unknown,
  C extends JsonRpcErrorCode | number = JsonRpcErrorCode | number,
> {
  code: C;
  message: string;
  data?: D;
}

export interface JsonRpcSuccessResponse<R = unknown> {
  jsonrpc: JsonRpcVersion;
  result: R;
  id: JsonRpcId;
}

export interface JsonRpcErrorResponse<D = unknown> {
  jsonrpc: JsonRpcVersion;
  error: JsonRpcErrorStructure<D>;
  id: JsonRpcId;
}

export type JsonRpcResponse<R = unknown, D = unknown> =
  JsonRpcSuccessResponse<R> | JsonRpcErrorResponse<D>;
