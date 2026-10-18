<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';

  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import Dialog from '#lib/components/Dialog.svelte';
  import LayerCard from '#lib/components/LayerCard.svelte';
  import { ApiError, describeError, request } from '#lib/api.js';
  import { ROLE_LABELS } from '#lib/format.js';
  import { confirmsDeletion, tenantNameError } from '#lib/invite-form.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { Tenant } from '#lib/types.js';
  import { fieldError, help, input, label } from '#lib/ui.js';

  const current = $derived(selectedTenant.current);
  const isOwner = $derived(selectedTenant.isOwner);

  let name = $state(selectedTenant.current?.name ?? '');
  let nameError = $state('');
  let renaming = $state(false);
  let error = $state('');
  let notice = $state('');

  let confirmingLeave = $state(false);
  let leaving = $state(false);
  let confirmingDelete = $state(false);
  let typedSlug = $state('');
  let deleting = $state(false);

  const canDelete = $derived(current ? confirmsDeletion(typedSlug, current.slug) : false);

  $effect(() => {
    if (!confirmingDelete) typedSlug = '';
  });

  function describe(err: unknown): string {
    if (err instanceof ApiError && err.message.includes('at least one owner')) {
      return 'Eres el único propietario: nombra otro en Miembros antes de salir.';
    }
    return describeError(err);
  }

  async function rename(event: SubmitEvent) {
    event.preventDefault();
    if (!current) return;
    nameError = tenantNameError(name) ?? '';
    if (nameError) return;
    renaming = true;
    error = '';
    notice = '';
    try {
      const renamed = await request<Tenant>('PUT', '/v1/tenant', {
        tenant: current.id,
        body: { name: name.trim() },
      });
      selectedTenant.select({ ...current, name: renamed.name });
      name = renamed.name;
      notice = 'Nombre guardado.';
    } catch (err) {
      error = describe(err);
    } finally {
      renaming = false;
    }
  }

  /** Out of this tenant (left or deleted): back to the organization picker. */
  function exit() {
    selectedTenant.clear();
    void goto(resolve('/'));
  }

  async function leave() {
    if (!current) return;
    leaving = true;
    error = '';
    try {
      await request<void>('POST', '/v1/tenant/leave', { tenant: current.id });
      confirmingLeave = false;
      exit();
    } catch (err) {
      confirmingLeave = false;
      error = describe(err);
    } finally {
      leaving = false;
    }
  }

  async function remove(event: SubmitEvent) {
    event.preventDefault();
    if (!current || !canDelete) return;
    deleting = true;
    error = '';
    try {
      // Needs a recent authentication: the api answers reauth_required and the request
      // client opens the password prompt, then retries.
      await request<void>('DELETE', '/v1/tenant', { tenant: current.id });
      confirmingDelete = false;
      exit();
    } catch (err) {
      confirmingDelete = false;
      error = describe(err);
    } finally {
      deleting = false;
    }
  }
</script>

<svelte:head>
  <title>Ajustes de la organización · Taut</title>
</svelte:head>

{#if error}<Banner tone="danger">{error}</Banner>{/if}
{#if notice}<Banner tone="success">{notice}</Banner>{/if}

<LayerCard title="Organización">
  <dl class="grid gap-3 text-sm sm:grid-cols-3">
    <div>
      <dt class="text-subtle">Nombre</dt>
      <dd class="font-medium text-default">{current?.name}</dd>
    </div>
    <div>
      <dt class="text-subtle">Slug</dt>
      <dd class="font-mono text-default">/{current?.slug}</dd>
    </div>
    <div>
      <dt class="text-subtle">Tu rol</dt>
      <dd class="font-medium text-default">{current ? ROLE_LABELS[current.role] : ''}</dd>
    </div>
  </dl>
</LayerCard>

{#if isOwner}
  <form onsubmit={rename} novalidate>
    <LayerCard title="Cambiar el nombre">
      <div class="space-y-4">
        <div>
          <label class={label} for="tenant-name">Nombre</label>
          <input
            id="tenant-name"
            class={input}
            bind:value={name}
            maxlength="100"
            aria-invalid={nameError ? 'true' : undefined}
          />
          {#if nameError}
            <p class={fieldError}>{nameError}</p>
          {:else}
            <p class={help}>El slug y la dirección de la página de estado no cambian.</p>
          {/if}
        </div>
        <Button type="submit" variant="primary" disabled={renaming}>
          {renaming ? 'Guardando…' : 'Guardar nombre'}
        </Button>
      </div>
    </LayerCard>
  </form>
{/if}

<LayerCard title="Salir de la organización">
  <div class="flex flex-wrap items-center justify-between gap-3">
    <p class="max-w-[60ch] text-sm text-subtle">
      Dejas de ver sus monitores e incidentes. Para volver necesitas otra invitación.
    </p>
    <Button variant="secondary-destructive" onclick={() => (confirmingLeave = true)}>Salir</Button>
  </div>
</LayerCard>

{#if isOwner}
  <LayerCard title="Eliminar la organización">
    <div class="flex flex-wrap items-center justify-between gap-3">
      <p class="max-w-[60ch] text-sm text-subtle">
        Borra para siempre los monitores, resultados, incidentes, canales, miembros e invitaciones,
        y la página de estado deja de existir.
      </p>
      <Button variant="destructive" onclick={() => (confirmingDelete = true)}>
        Eliminar organización
      </Button>
    </div>
  </LayerCard>
{/if}

<Dialog
  bind:open={confirmingLeave}
  alert
  title="¿Salir de «{current?.name ?? ''}»?"
  description="Perderás el acceso de inmediato."
>
  {#snippet actions()}
    <Button variant="ghost" onclick={() => (confirmingLeave = false)}>Cancelar</Button>
    <Button variant="destructive" onclick={leave} disabled={leaving}>
      {leaving ? 'Saliendo…' : 'Salir'}
    </Button>
  {/snippet}
</Dialog>

<Dialog
  bind:open={confirmingDelete}
  alert
  title="¿Eliminar «{current?.name ?? ''}»?"
  description="Esto no se puede deshacer. Si entraste hace más de 5 minutos, te pediremos la contraseña."
>
  <form id="delete-tenant" onsubmit={remove} novalidate>
    <label class={label} for="delete-slug">
      Escribe <code class="font-mono text-[0.9em] text-strong">{current?.slug}</code> para confirmar
    </label>
    <input
      id="delete-slug"
      class={[input, 'font-mono']}
      autocomplete="off"
      spellcheck="false"
      bind:value={typedSlug}
    />
  </form>
  {#snippet actions()}
    <Button variant="ghost" onclick={() => (confirmingDelete = false)}>Cancelar</Button>
    <Button
      type="submit"
      form="delete-tenant"
      variant="destructive"
      disabled={!canDelete || deleting}
    >
      {deleting ? 'Eliminando…' : 'Eliminar para siempre'}
    </Button>
  {/snippet}
</Dialog>
