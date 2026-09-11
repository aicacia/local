<script lang="ts" module>
    const RESPONSE_TYPES = ["code"] as const;
    const RESPONSE_MODES = ["query"] as const;
</script>

<script lang="ts">
    import type {
        AuthorizationRequest,
        CodeChallengeMethod,
    } from "@aicacia/idp-client";
    import { page } from "$app/state";
    import { m } from "$lib/paraglide/messages";
    import Authorize from "./_Authorize.svelte";

    import { rejectAuthorizationRequest } from "./_utils";

    let { data } = $props();

    let userInfo = $derived(data.userInfo);

    let urlClientId = $derived(page.url.searchParams.get("client_id"));
    let urlResponseType = $derived(page.url.searchParams.get("response_type"));
    let urlResponseMode = $derived(page.url.searchParams.get("response_mode"));
    let urlScope = $derived(page.url.searchParams.get("scope"));
    let urlRedirectUri = $derived(page.url.searchParams.get("redirect_uri"));
    let urlState = $derived(page.url.searchParams.get("state") ?? undefined);
    let urlNonce = $derived(page.url.searchParams.get("nonce") ?? undefined);
    let urlCodeChallenge = $derived(
        page.url.searchParams.get("code_challenge") ?? undefined,
    );
    let urlRegistration = $derived(
        page.url.searchParams.get("registration") ?? undefined,
    );
    let urlCodeChallengeMethod = $derived<CodeChallengeMethod | undefined>(
        (page.url.searchParams.get(
            "code_challenge_method",
        ) as CodeChallengeMethod) ?? undefined,
    );

    let responseTypeError = $state<string>();
    let responseModeError = $state<string>();
    let scopeError = $state<string>();
    let redirectUriError = $state<string>();

    let authorizationRequest = $state<AuthorizationRequest>();

    $effect(() => {
        if (!urlResponseType) {
            responseTypeError = m.authorize_response_type_required();
        } else if (!RESPONSE_TYPES.includes(urlResponseType as never)) {
            responseTypeError = m.authorize_response_type_invalid({
                value: urlResponseType,
                allowed: RESPONSE_TYPES.join(","),
            });
        } else {
            responseTypeError = undefined;
        }
        if (!urlResponseMode) {
            responseModeError = m.authorize_response_mode_required();
        } else if (!RESPONSE_MODES.includes(urlResponseMode as never)) {
            responseModeError = m.authorize_response_mode_invalid({
                value: urlResponseMode,
                allowed: RESPONSE_MODES.join(","),
            });
        } else {
            responseModeError = undefined;
        }
        if (!urlRedirectUri) {
            redirectUriError = m.authorize_redirect_uri_required();
        } else {
            redirectUriError = undefined;
        }
        if (!urlScope) {
            scopeError = m.authorize_scope_required();
        } else {
            scopeError = undefined;
        }
        if (
            responseTypeError ||
            responseModeError ||
            redirectUriError ||
            scopeError
        ) {
            authorizationRequest = undefined;
            return;
        }
        authorizationRequest = {
            clientId: urlClientId ?? "unknown",
            responseType: urlResponseType as (typeof RESPONSE_TYPES)[number],
            responseMode: urlResponseMode as (typeof RESPONSE_MODES)[number],
            redirectUri: urlRedirectUri,
            scope: urlScope,
            state: urlState,
            nonce: urlNonce,
            codeChallenge: urlCodeChallenge,
            codeChallengeMethod: urlCodeChallengeMethod,
        };
    });

    function onReject() {
        if (!urlRedirectUri) {
            window.close();
            return;
        }
        rejectAuthorizationRequest(
            {
                redirectUri: urlRedirectUri,
                state: urlState,
                nonce: urlNonce,
            },
            "invalid_request",
            "The request is missing a required parameter, includes an invalid parameter value, includes a parameter more than once, or is otherwise malformed.",
        );
    }
</script>

<div class="overflow-auto">
    <div class="m-8 flex grow flex-col items-center justify-center">
        <div class="card w-md">
            {#if authorizationRequest}
                <Authorize
                    {userInfo}
                    {authorizationRequest}
                    registration={urlRegistration}
                />
            {:else}
                <section>
                    <h5>{m.authorize_invalid_request()}</h5>
                    <ul class="list-inside list-disc space-y-1 text-sm">
                        {#if responseTypeError}
                            <li>
                                <strong
                                    >{m.authorize_invalid_response_type_label()}</strong
                                >
                                {responseTypeError}
                            </li>
                        {/if}
                        {#if responseModeError}
                            <li>
                                <strong
                                    >{m.authorize_invalid_response_mode_label()}</strong
                                >
                                {responseModeError}
                            </li>
                        {/if}
                        {#if redirectUriError}
                            <li>
                                <strong
                                    >{m.authorize_invalid_redirect_uri_label()}</strong
                                >
                                {redirectUriError}
                            </li>
                        {/if}
                        {#if scopeError}
                            <li>
                                <strong
                                    >{m.authorize_invalid_scope_label()}</strong
                                >
                                {scopeError}
                            </li>
                        {/if}
                    </ul>
                    <div>
                        <div class="mt-4 flex flex-row justify-center gap-4">
                            <button
                                type="button"
                                class="btn secondary"
                                onclick={onReject}
                                >{m.authorize_button_deny()}</button
                            >
                        </div>
                    </div>
                </section>
            {/if}
        </div>
    </div>
</div>
