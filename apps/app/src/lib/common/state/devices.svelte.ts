import type {
  DeviceEnrollment,
  DeviceInfo,
  DeviceState,
} from "@aicacia/idp-client";
import { getOidcClient } from "./oidc.svelte";
import { getLidpApiUrl, lidpApi } from "./lidpClient.svelte";

export type { DeviceEnrollment, DeviceInfo, DeviceState };

export function listDevices(): Promise<DeviceInfo[]> {
  return lidpApi.listDevices();
}

export function enrollDevice(
  name: string,
  publicKey: string,
  address: string,
): Promise<DeviceEnrollment> {
  return lidpApi.enrollDevice({
    deviceEnrollmentRequest: { name, publicKey, address },
  });
}

export function requestDevicePairing(
  name: string,
  publicKey: string,
  address: string,
  acceptingPublicKey: string,
): Promise<DeviceEnrollment> {
  return lidpApi.requestPairing({
    devicePairingRequest: { name, publicKey, address, acceptingPublicKey },
  });
}
export async function getPairingAccepting(
  accepting?: boolean,
): Promise<boolean> {
  const baseUrl = getLidpApiUrl();
  if (!baseUrl) {
    throw new Error("LIdP API URL is not available");
  }
  const token = getOidcClient()?.getStoredTokenResponse()?.access_token;
  const response = await fetch(`${baseUrl}/devices/pairing-accepting`, {
    method: accepting === undefined ? "GET" : "PUT",
    headers: {
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...(accepting === undefined
        ? {}
        : { "Content-Type": "application/json" }),
    },
    body: accepting === undefined ? undefined : JSON.stringify({ accepting }),
  });
  if (!response.ok) {
    throw new Error("Could not update pairing mode");
  }
  return ((await response.json()) as { accepting: boolean }).accepting;
}

export async function getDeviceApprovalPayload(id: number): Promise<string> {
  return (await lidpApi.pairingApprovalPayload({ id })).payload;
}

export function approveDevice(
  id: number,
  signature: string,
): Promise<DeviceInfo> {
  return lidpApi.approveDevice({
    id,
    devicePairingApprovalRequest: { signature },
  });
}

export function renameDevice(id: number, name: string): Promise<DeviceInfo> {
  return lidpApi.updateDevice({ id, updateDeviceRequest: { name } });
}

export function revokeDevice(id: number): Promise<void> {
  return lidpApi.revokeDevice({ id });
}
