<script lang="ts" module>
    import "./layout.css";
</script>

<script lang="ts">
    import { getTheme } from "@aicacia/svelte-headless";
    import { onMount } from "svelte";
    import { resolve } from "$app/paths";
    import favicon from "$lib/assets/favicon.svg";
    import Notifications from "$lib/common/components/Notifications.svelte";
    import { handleDeepLink } from "$lib/common/util/handleDeepLink";
    import type { LayoutProps } from "./$types";
    import { onOpenUrl } from "@tauri-apps/plugin-deep-link";
    import type { UnlistenFn } from "@tauri-apps/api/event";

    let { children }: LayoutProps = $props();

    $effect(() => {
        if (getTheme() === "dark") {
            document.body.classList.add("dark");
            return;
        }

        document.body.classList.remove("dark");
    });

    onMount(() => {
        document.body.classList.add("hydrated");

        let onOpenUrlUnlistenFn: UnlistenFn | undefined;

        onOpenUrl(handleDeepLink).then((unlisten) => {
            onOpenUrlUnlistenFn = unlisten;
        });

        return () => {
            if (onOpenUrlUnlistenFn) {
                onOpenUrlUnlistenFn();
            }
        };
    });
</script>

<svelte:head>
    <link rel="icon" href={favicon} />
    <link
        rel="manifest"
        crossorigin="use-credentials"
        href={resolve("/manifest.json")}
    />
</svelte:head>

{@render children()}
<Notifications />
