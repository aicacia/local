import type {
    DeviceEnrollment,
    DeviceInfo,
    DevicePairingInvitation,
    UserDeviceState,
} from "@aicacia/lidp-client";
import { lidpApi } from "./lidpClient.svelte";

export type DeviceState = UserDeviceState;
export type { DeviceEnrollment, DeviceInfo };
export type DeviceInvitation = DevicePairingInvitation;

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

export function createDeviceInvitation(
    initiatingPublicKey: string,
): Promise<DeviceInvitation> {
    return lidpApi.createPairingInvitation({
        devicePairingInvitationRequest: { initiatingPublicKey },
    });
}

export function redeemDeviceInvitation(
    secret: string,
    name: string,
    publicKey: string,
    address: string,
): Promise<DeviceEnrollment> {
    return lidpApi.redeemPairingInvitation({
        devicePairingRedemptionRequest: { secret, name, publicKey, address },
    });
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
