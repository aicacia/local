<script lang="ts">
	import {
		handleNativeCallbackRequest,
		type NativeRequest,
	} from "@aicacia/native-fetch";
	import { page } from "$app/state";
	import ClientUpdates from "$lib/common/components/client/ClientUpdates.svelte";
	import { camelizeKeys } from "$lib/common/util/camelizeKeys";
	import { createChangedClientRegisterFields } from "$lib/common/util/clientRegisterDiff";
	import { clientClient, oidcClient } from "$lib/common/util/grpcClient";
	import { redirectToUrl } from "$lib/common/util/redirectToUrl";
	import { snakeCaseKeys } from "$lib/common/util/snakeCaseKeys";
	import { m } from "$lib/paraglide/messages";
	import type { Client, ClientRegisterRequest } from "$lib/proto/mises.js";

	function createRedirectClient(existing: Client): Client {
		return {
			...existing,
			serviceId: existing.serviceId ?? undefined,
		};
	}
	let urlRegistration = $derived(
		page.url.searchParams.get("registration") ?? undefined,
	);
	let urlRedirect = $derived(
		page.url.searchParams.get("redirect_uri") ?? undefined,
	);
	let urlState = $derived(page.url.searchParams.get("state") ?? undefined);
	let nativeRequest = $derived.by(() => {
		const native = page.url.searchParams.get("native");
		if (!native) {
			return undefined;
		}

		try {
			return JSON.parse(native) as NativeRequest;
		} catch {
			return undefined;
		}
	});

	let clientRegisterRequest = $derived.by(() => {
		if (urlRegistration) {
			try {
				return camelizeKeys(
					JSON.parse(urlRegistration),
				) as ClientRegisterRequest;
			} catch {
				return null;
			}
		} else {
			return null;
		}
	});
	let loading = $state(false);
	let existingClients = $state<Client[]>([]);
	let selectedClientId = $state<string | null>(null);

	let selectedClient = $derived.by(
		() =>
			existingClients.find((client) => client.id === selectedClientId) ?? null,
	);
	let selectedClientDiff = $derived.by(() => {
		if (!clientRegisterRequest || !selectedClient) {
			return null;
		}

		return createChangedClientRegisterFields(
			clientRegisterRequest,
			selectedClient,
		);
	});

	$effect(() => {
		let cancelled = false;

		async function loadExistingClients() {
			if (!clientRegisterRequest?.serviceId) {
				existingClients = [];
				selectedClientId = null;
				return;
			}

			try {
				const response = await clientClient().listByService({
					serviceId: clientRegisterRequest.serviceId,
				});
				if (cancelled) {
					return;
				}

				existingClients = response.clients ?? [];
				selectedClientId = null;
			} catch (e) {
				if (cancelled) {
					return;
				}

				console.error("Failed to list existing clients for service", e);
				existingClients = [];
				selectedClientId = null;
			}
		}

		void loadExistingClients();

		return () => {
			cancelled = true;
		};
	});

	async function onAccept() {
		if (!clientRegisterRequest) {
			return;
		}

		if (!nativeRequest && !urlRedirect) {
			window.close();
			return;
		}

		loading = true;
		try {
			let client: Client;

			if (selectedClient) {
				if (!selectedClientDiff) {
					client = createRedirectClient(selectedClient);
				} else {
					client = await oidcClient().clientRegister({
						clientId: selectedClient.id,
						serviceId:
							clientRegisterRequest.serviceId ?? selectedClient.serviceId,
						...selectedClientDiff,
					});
				}
			} else {
				client = await oidcClient().clientRegister(clientRegisterRequest);
			}

			if (nativeRequest) {
				await redirectToUrl(
					await handleNativeCallbackRequest(
						nativeRequest,
						() => new Response(JSON.stringify(snakeCaseKeys(client))),
					),
				);
			} else {
				const u = new URL(urlRedirect!);
				u.searchParams.set("client", JSON.stringify(client));
				if (urlState) {
					u.searchParams.set("state", urlState);
				}
				await redirectToUrl(u);
			}
		} catch (e) {
			console.error("Error registering client", e);
		} finally {
			loading = false;
		}
	}

	async function onReject() {
		if (!nativeRequest && !urlRedirect) {
			window.close();
			return;
		}

		if (nativeRequest) {
			await redirectToUrl(
				await handleNativeCallbackRequest(
					nativeRequest,
					() =>
						new Response(
							JSON.stringify({
								error: "registration_denied",
							}),
						),
				),
			);
		} else {
			const u = new URL(urlRedirect!);
			u.searchParams.set("error", "registration_denied");
			if (urlState) {
				u.searchParams.set("state", urlState);
			}
			await redirectToUrl(u);
		}
	}
</script>

<div class="overflow-auto">
	<div class="m-8 flex grow flex-col items-center justify-center">
		<div class="card w-md">
			{#if clientRegisterRequest}
				{#if existingClients.length > 0}
					<section class="mb-4">
						<label for="existing-client-id"
							>{m.register_existing_client_selection_label()}</label
						>
						<select
							id="existing-client-id"
							class="max-w-full"
							bind:value={selectedClientId}
						>
							<option value={null}
								>{m.register_create_new_client_option()}</option
							>
							{#each existingClients as client (client.id)}
								<option value={client.id}>
									{client.name ?? client.clientId} ({client.clientId})
								</option>
							{/each}
						</select>
					</section>
				{/if}

				<ClientUpdates
					client={clientRegisterRequest}
					changedFields={selectedClientDiff}
					isNew={!selectedClientId}
					disabled={loading}
					{onAccept}
					{onReject}
				/>
			{:else}
				<section>
					<h5>{m.register_invalid_request_title()}</h5>
					<p>{m.register_invalid_request_description()}</p>
					<div class="mt-4 flex flex-row justify-center gap-4">
						<button class="btn secondary" onclick={onReject}
							>{m.client_close()}</button
						>
					</div>
				</section>
			{/if}
		</div>
	</div>
</div>
