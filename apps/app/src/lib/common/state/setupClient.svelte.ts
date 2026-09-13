import { invoke, isTauri } from "@tauri-apps/api/core";
import { ensureTauriIdpApiUrl } from "./idpClient.svelte";

export type SetupStage = "installation" | "device" | "ready";

type SetupStatus = {
  stage: SetupStage;
};

type SetupNewInput = {
  deviceName: string;
  adminUsername: string;
  adminPassword: string;
};

type SetupJoinInput = {
  deviceName: string;
  endpointAddr: string;
};

export type SetupResidency = "full" | "passthrough";

type SetupDeviceStatus = {
  residency: SetupResidency;
};

async function setupUrl(path: string): Promise<string> {
  if (!isTauri()) {
    throw new Error("Setup is only available in the desktop app");
  }

  const baseUrl = await ensureTauriIdpApiUrl();
  if (!baseUrl) {
    throw new Error("Local IdP URL is not available");
  }
  return `${baseUrl}${path}`;
}

async function setupToken(): Promise<string> {
  const token = await invoke<string | null>("get_setup_token");
  if (!token) {
    throw new Error("Setup is already complete");
  }
  return token;
}

async function request(path: string, init?: RequestInit): Promise<Response> {
  return fetch(await setupUrl(path), init);
}

export async function getSetupStage(): Promise<SetupStage> {
  const response = await request("/setup/status");
  if (!response.ok) {
    throw new Error("Could not read setup status");
  }
  return ((await response.json()) as SetupStatus).stage;
}

export async function setupNew(input: SetupNewInput): Promise<void> {
  const response = await request("/setup/new", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-setup-token": await setupToken(),
    },
    body: JSON.stringify(input),
  });
  if (!response.ok) {
    throw new Error("Could not create the installation");
  }
}

export async function setupJoin(input: SetupJoinInput): Promise<void> {
  const response = await request("/setup/join", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-setup-token": await setupToken(),
    },
    body: JSON.stringify(input),
  });
  if (!response.ok) {
    throw new Error("Could not start joining the installation");
  }
}

export async function getDeviceResidency(): Promise<SetupResidency> {
  const response = await request("/setup/device/residency", {
    headers: { "x-setup-token": await setupToken() },
  });
  if (!response.ok) {
    throw new Error("Could not read device storage settings");
  }
  return ((await response.json()) as SetupDeviceStatus).residency;
}

export async function setDeviceResidency(
  residency: SetupResidency,
): Promise<void> {
  const response = await request("/setup/device/residency", {
    method: "PUT",
    headers: {
      "content-type": "application/json",
      "x-setup-token": await setupToken(),
    },
    body: JSON.stringify({ residency }),
  });
  if (!response.ok) {
    throw new Error("Could not save device storage settings");
  }
}

export async function completeDeviceSetup(): Promise<void> {
  const response = await request("/setup/device", {
    method: "POST",
    headers: { "x-setup-token": await setupToken() },
  });
  if (!response.ok) {
    throw new Error("Could not complete device setup");
  }
}
