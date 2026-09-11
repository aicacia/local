<script lang="ts">
  import { resolve } from "$app/paths";
</script>

<div class="flex flex-col gap-4">
  <h1 class="mb-0 text-4xl">Clients</h1>
  <div class="card secondary flex flex-col gap-3">
    <h2 class="mb-0 text-2xl">Clients are managed per application</h2>
    <p class="mb-0 text-sm opacity-80">
      Open an application to list and create clients in that application.
    </p>
    <div>
      <a class="btn primary" href={resolve("/applications")}
        >Go to applications</a
      >
    </div>
  </div>
</div>
