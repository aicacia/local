<script lang="ts">
    import type { Snippet } from "svelte";

    interface Props {
        title: string;
        children: Snippet;
    }

    let { title, children }: Props = $props();
    let dialog = $state<HTMLDialogElement>();

    export function show() {
        dialog?.showModal();
    }

    export function close() {
        dialog?.close();
    }
</script>

<dialog
    bind:this={dialog}
    class="m-auto w-full max-w-lg rounded-xl bg-white p-5 text-gray-950 shadow-xl backdrop:bg-black/50 dark:bg-gray-950 dark:text-white"
    aria-labelledby="modal-title"
>
    <div class="flex flex-col gap-4">
        <div class="flex items-center justify-between gap-4">
            <h2 id="modal-title" class="mb-0 text-2xl">{title}</h2>
            <form method="dialog">
                <button type="submit" class="btn secondary" aria-label="Close"
                    >Close</button
                >
            </form>
        </div>
        {@render children()}
    </div>
</dialog>
