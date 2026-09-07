import { getLidpApiUrl } from "./lidpClient.svelte";
import { getOidcClient } from "./oidc.svelte";

export type DeviceState = "pending" | "approved" | "revoked";

export interface DeviceInfo {
  id: number;
  name: string;
  publicKey: string;
  address: string;
  state: DeviceState;
  createdAt: number;
  updatedAt: number;
  revokedAt?: number;
}

export interface DeviceEnrollment {
  id: number;
  state: DeviceState;
}

export interface DeviceInvitation {
  id: number;
  secret: string;
  expiresAt: number;
}

interface DeviceApprovalPayload {
  payload: string;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const baseUrl = getLidpApiUrl();
  const token = getOidcClient().getStoredTokenResponse()?.access_token;
  if (!baseUrl || !token) {
    throw new Error("Sign in and configure the LIdP API first");
  }

  const response = await fetch(`${baseUrl.replace(/\/$/, "")}${path}`, {
    ...init,
    headers: {
      Authorization: `Bearer ${token}`,
      ...(init?.body ? { "Content-Type": "application/json" } : {}),
      ...init?.headers,
    },
  });
  if (!response.ok) {
    throw new Error(`Device request failed (${response.status})`);
  }
  return response.status === 204
    ? (undefined as T)
    : ((await response.json()) as T);
}

export function listDevices(): Promise<DeviceInfo[]> {
  return request("/devices");
}

export function enrollDevice(
  name: string,
  publicKey: string,
  address: string,
): Promise<DeviceEnrollment> {
  return request("/devices/enrollments", {
    method: "POST",
    body: JSON.stringify({ name, publicKey, address }),
  });
}

export function createDeviceInvitation(
  initiatingPublicKey: string,
): Promise<DeviceInvitation> {
  return request("/devices/pairing-invitations", {
    method: "POST",
    body: JSON.stringify({ initiatingPublicKey }),
  });
}

export function redeemDeviceInvitation(
  secret: string,
  name: string,
  publicKey: string,
  address: string,
): Promise<DeviceEnrollment> {
  return request("/devices/pairing-invitations/redeem", {
    method: "POST",
    body: JSON.stringify({ secret, name, publicKey, address }),
  });
}

export async function getDeviceApprovalPayload(id: number): Promise<string> {
  const result = await request<DeviceApprovalPayload>(
    `/devices/enrollments/${id}/approval-payload`,
  );
  return result.payload;
}

export function approveDevice(
  id: number,
  signature: string,
): Promise<DeviceInfo> {
  return request(`/devices/enrollments/${id}/approve`, {
    method: "POST",
    body: JSON.stringify({ signature }),
  });
}

export function renameDevice(id: number, name: string): Promise<DeviceInfo> {
  return request(`/devices/${id}`, {
    method: "PATCH",
    body: JSON.stringify({ name }),
  });
}

export function revokeDevice(id: number): Promise<void> {
  return request(`/devices/${id}`, { method: "DELETE" });
}
