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
    import { isTauri } from "@tauri-apps/api/core";

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

        if (!isTauri()) {
            return;
        }

        let onOpenUrlUnlistenFn: (() => void) | undefined;

        import("@tauri-apps/plugin-deep-link").then(({ onOpenUrl }) =>
            onOpenUrl(handleDeepLink).then((unlisten) => {
                onOpenUrlUnlistenFn = unlisten;
            }),
        );

        return () => {
            onOpenUrlUnlistenFn?.();
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
