/* tslint:disable */
/* eslint-disable */

import { mapValues } from "../runtime.js";
import type { DeviceState } from "./DeviceState.js";
import {
  DeviceStateFromJSON,
  DeviceStateFromJSONTyped,
  DeviceStateToJSON,
  DeviceStateToJSONTyped,
} from "./DeviceState.js";

export interface DeviceEnrollment {
  approvalCode?: string | null;
  id: number;
  state: DeviceState;
}

export function instanceOfDeviceEnrollment(
  value: object,
): value is DeviceEnrollment {
  if (!("id" in value) || value["id"] === undefined) return false;
  if (!("state" in value) || value["state"] === undefined) return false;
  return true;
}

export function DeviceEnrollmentFromJSON(json: any): DeviceEnrollment {
  return DeviceEnrollmentFromJSONTyped(json, false);
}

export function DeviceEnrollmentFromJSONTyped(
  json: any,
  ignoreDiscriminator: boolean,
): DeviceEnrollment {
  if (json == null) return json;
  return {
    approvalCode:
      json["approvalCode"] === undefined
        ? undefined
        : json["approvalCode"] === null
          ? null
          : json["approvalCode"],
    id: json["id"],
    state: DeviceStateFromJSON(json["state"]),
  };
}

export function DeviceEnrollmentToJSON(json: any): DeviceEnrollment {
  return DeviceEnrollmentToJSONTyped(json, false);
}

export function DeviceEnrollmentToJSONTyped(
  value?: DeviceEnrollment | null,
  ignoreDiscriminator: boolean = false,
): any {
  if (value == null) return value;
  return {
    approvalCode: value["approvalCode"],
    id: value["id"],
    state: DeviceStateToJSON(value["state"]),
  };
}
