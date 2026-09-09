/* tslint:disable */
/* eslint-disable */

export const DeviceState = {
  Pending: "pending",
  Approved: "approved",
  Revoked: "revoked",
} as const;

export type DeviceState = (typeof DeviceState)[keyof typeof DeviceState];

export function instanceOfDeviceState(value: any): boolean {
  for (const key in DeviceState) {
    if (Object.prototype.hasOwnProperty.call(DeviceState, key)) {
      if (DeviceState[key as keyof typeof DeviceState] === value) return true;
    }
  }
  return false;
}

export function DeviceStateFromJSON(json: any): DeviceState {
  return DeviceStateFromJSONTyped(json, false);
}

export function DeviceStateFromJSONTyped(
  json: any,
  ignoreDiscriminator: boolean,
): DeviceState {
  return json as DeviceState;
}

export function DeviceStateToJSON(value?: DeviceState | null): any {
  return value as any;
}

export function DeviceStateToJSONTyped(
  value: any,
  ignoreDiscriminator: boolean,
): DeviceState {
  return value as DeviceState;
}
