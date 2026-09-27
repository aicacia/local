import { isTauri } from "@tauri-apps/api/core";
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
  idpUrl: string;
  username: string;
  password: string;
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
    headers: { "content-type": "application/json" },
    body: JSON.stringify(input),
  });
  if (!response.ok) {
    throw new Error("Could not create the installation");
  }
}

export async function setupJoin(input: SetupJoinInput): Promise<void> {
  const idpUrl = input.idpUrl.replace(/\/$/, "");
  const tokenResponse = await fetch(`${idpUrl}/oauth2/token`, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({
      grant_type: "password",
      client_id: "management-desktop",
      username: input.username,
      password: input.password,
      scope: "openid profile email setup",
    }),
  });
  if (!tokenResponse.ok) {
    throw new Error("Could not authenticate with the existing installation");
  }
  const token = (await tokenResponse.json()) as { access_token?: string };
  if (!token.access_token) {
    throw new Error("Existing installation returned no access token");
  }

  const response = await request("/setup/join", {
    method: "POST",
    headers: {
      authorization: `Bearer ${token.access_token}`,
      "content-type": "application/json",
    },
    body: JSON.stringify({ deviceName: input.deviceName, idpUrl }),
  });
  if (!response.ok) {
    throw new Error("Could not join the existing installation");
  }
}

export async function getDeviceResidency(): Promise<SetupResidency> {
  const response = await request("/setup/device/residency");
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
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ residency }),
  });
  if (!response.ok) {
    throw new Error("Could not save device storage settings");
  }
}

export async function completeDeviceSetup(): Promise<void> {
  const response = await request("/setup/device", { method: "POST" });
  if (!response.ok) {
    throw new Error("Could not complete device setup");
  }
}
