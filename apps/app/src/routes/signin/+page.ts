import { redirect } from "@sveltejs/kit";
import { isTauri } from "@tauri-apps/api/core";
import { getSetupStage } from "$lib/common/state/setupClient.svelte";

export async function load() {
  if (isTauri() && (await getSetupStage()) !== "ready") {
    throw redirect(302, "/setup");
  }
}
