<script lang="ts" module>
    import * as v from "valibot";
    import { m } from "$lib/paraglide/messages";
    import { validateLidpApiUrl } from "$lib/common/state/lidpClient.svelte";
    import { validateLidpManagementApiUrl } from "$lib/common/state/lidpManagementClient.svelte";

    const configSchema = v.objectAsync({
        lidpApiUrl: v.pipeAsync(
            v.string(),
            v.nonEmpty(m.errors_message_required()),
            v.url(m.errors_message_invalid_url()),
            v.checkAsync(validateLidpApiUrl, m.errors_message_invalid_url()),
        ),
        lidpManagementApiUrl: v.pipeAsync(
            v.string(),
            v.nonEmpty(m.errors_message_required()),
            v.url(m.errors_message_invalid_url()),
            v.checkAsync(
                validateLidpManagementApiUrl,
                m.errors_message_invalid_url(),
            ),
        ),
    });
</script>

<script lang="ts">
    import { createForm } from "@aicacia/svelte-forms";
    import Issues from "$lib/common/components/Issues.svelte";
    import {
        getLidpApiUrl,
        setLidpApiUrl,
    } from "$lib/common/state/lidpClient.svelte";
    import {
        getLidpManagementApiUrl,
        setLidpManagementApiUrl,
    } from "$lib/common/state/lidpManagementClient.svelte";
    import { notifications } from "$lib/common/state/notifications.svelte";

    interface Props {
        onSaved?: () => void;
    }

    let { onSaved = () => {} }: Props = $props();

    const form = createForm(configSchema, {
        lidpApiUrl: getLidpApiUrl() ?? "",
        lidpManagementApiUrl: getLidpManagementApiUrl() ?? "",
    });

    export function reset() {
        form.fields.lidpApiUrl.value = getLidpApiUrl() ?? "";
        form.fields.lidpManagementApiUrl.value =
            getLidpManagementApiUrl() ?? "";
    }

    async function onSubmit(event: SubmitEvent) {
        event.preventDefault();

        const [_input, output, error] = await form.validate();
        if (error) {
            notifications.add(m.errors_message_invalid_form(), "error");
            return;
        }

        setLidpApiUrl(output.lidpApiUrl);
        setLidpManagementApiUrl(output.lidpManagementApiUrl);
        onSaved();
    }
</script>

<form onsubmit={onSubmit} class="flex flex-col gap-4">
    <label class="flex flex-col">
        {m.env_config_lipd_url()}
        <input
            type="text"
            aria-label={m.env_config_lipd_url()}
            placeholder={m.env_config_lipd_url_placeholder()}
            bind:value={form.fields.lidpApiUrl.value}
        />
        <Issues issues={form.fields.lidpApiUrl.issues} />
    </label>
    <label class="flex flex-col">
        {m.env_config_lipd_management_url()}
        <input
            type="text"
            aria-label={m.env_config_lipd_management_url()}
            placeholder={m.env_config_lipd_management_url_placeholder()}
            bind:value={form.fields.lidpManagementApiUrl.value}
        />
        <Issues issues={form.fields.lidpManagementApiUrl.issues} />
    </label>
    <input type="submit" value={m.env_config_save()} class="btn primary" />
</form>
