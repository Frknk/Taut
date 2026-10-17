<script lang="ts">
  import { onMount } from 'svelte';

  import Badge from '#lib/components/Badge.svelte';
  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import Dialog from '#lib/components/Dialog.svelte';
  import Empty from '#lib/components/Empty.svelte';
  import LayerCard from '#lib/components/LayerCard.svelte';
  import Select from '#lib/components/Select.svelte';
  import { API_URL, describeError, request } from '#lib/api.js';
  import {
    SCOPE_HINTS,
    SCOPE_LABELS,
    curlExample,
    emptyApiKey,
    toApiKeyInput,
    validateApiKey,
    type ApiKeyErrors,
  } from '#lib/api-key-form.js';
  import { formatDateTime, timeAgo } from '#lib/format.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { ApiKey, ApiKeyScope, CreatedApiKey } from '#lib/types.js';
  import { fieldError, help, input, label, table } from '#lib/ui.js';

  const tenantId = $derived(selectedTenant.current?.id ?? '');
  const scopes = Object.keys(SCOPE_LABELS) as ApiKeyScope[];
  const formId = $props.id();

  let keys = $state<ApiKey[]>([]);
  let loaded = $state(false);
  let error = $state('');
  let notice = $state('');

  // Creating: the form first, then the new key, shown this one time.
  let creating = $state(false);
  let values = $state(emptyApiKey());
  let errors = $state<ApiKeyErrors>({});
  let createError = $state('');
  let saving = $state(false);
  let created = $state<CreatedApiKey | null>(null);
  let copied = $state<'key' | 'curl' | null>(null);
  let copyFailed = $state(false);

  let revoking = $state<ApiKey | null>(null);
  let confirmingRevoke = $state(false);

  // Closing the dialog any other way (Escape) also forgets the key.
  $effect(() => {
    if (!creating) created = null;
  });

  async function load() {
    try {
      keys = await request<ApiKey[]>('GET', '/v1/api-keys', { tenant: tenantId });
      error = '';
    } catch (err) {
      error = describeError(err);
    } finally {
      loaded = true;
    }
  }

  function openCreate() {
    values = emptyApiKey();
    errors = {};
    createError = '';
    created = null;
    copied = null;
    copyFailed = false;
    notice = '';
    creating = true;
  }

  async function create(event: SubmitEvent) {
    event.preventDefault();
    errors = validateApiKey(values);
    if (Object.keys(errors).length > 0) return;
    saving = true;
    createError = '';
    try {
      // A stale session answers `reauth_required`: the API client asks for the password and
      // retries on its own.
      created = await request<CreatedApiKey>('POST', '/v1/api-keys', {
        tenant: tenantId,
        body: toApiKeyInput(values),
      });
      await load();
    } catch (err) {
      createError = describeError(err);
    } finally {
      saving = false;
    }
  }

  async function copy(what: 'key' | 'curl') {
    if (!created) return;
    const text = what === 'key' ? created.key : curlExample(API_URL, created.key);
    try {
      await navigator.clipboard.writeText(text);
      copied = what;
      copyFailed = false;
      setTimeout(() => (copied = null), 2000);
    } catch {
      // The browser refused the clipboard: the text stays selectable with one click.
      copied = null;
      copyFailed = true;
    }
  }

  function askRevoke(key: ApiKey) {
    revoking = key;
    confirmingRevoke = true;
  }

  async function revoke(key: ApiKey) {
    confirmingRevoke = false;
    try {
      await request<void>('DELETE', `/v1/api-keys/${key.id}`, { tenant: tenantId });
      notice = `API key «${key.name}» revocada: deja de funcionar ya.`;
      await load();
    } catch (err) {
      error = describeError(err);
    }
  }

  onMount(() => {
    if (selectedTenant.isOwner) load();
  });
</script>

<svelte:head>
  <title>API keys · Taut</title>
</svelte:head>

{#if !selectedTenant.isOwner}
  <Empty
    title="Solo para propietarios"
    text="Las API keys dan acceso a la organización sin iniciar sesión; solo los propietarios las crean y revocan."
  />
{:else}
  {#if error}<Banner tone="danger">{error}</Banner>{/if}
  {#if notice}<Banner tone="success">{notice}</Banner>{/if}

  <LayerCard title="API keys de {selectedTenant.current?.name}" flush>
    {#snippet actions()}
      <Button size="sm" variant="primary" onclick={openCreate}>Nueva API key</Button>
    {/snippet}
    {#if loaded && keys.length === 0}
      <div class="p-4">
        <Empty
          title="Todavía no hay API keys"
          text="Crea una para que un script, Terraform o tu CI consulten y gestionen monitores."
        />
      </div>
    {:else if keys.length > 0}
      <table class={table.root}>
        <thead>
          <tr>
            <th class={table.th}>Nombre</th>
            <th class={table.th}>Clave</th>
            <th class={table.th}>Permiso</th>
            <th class={table.th}>Creada</th>
            <th class={table.th}>Último uso</th>
            <th class={table.th}><span class="sr-only">Acciones</span></th>
          </tr>
        </thead>
        <tbody>
          {#each keys as key (key.id)}
            <tr class={table.row}>
              <td class={[table.td, key.revoked_at ? 'text-subtle' : 'font-medium text-strong']}>
                {key.name}
              </td>
              <td class={table.td}><code class="font-mono text-xs">{key.prefix}…</code></td>
              <td class={table.td}>
                <Badge tone={key.scope === 'write' ? 'inverted' : 'outline'}>
                  {SCOPE_LABELS[key.scope]}
                </Badge>
              </td>
              <td class={[table.td, 'text-subtle']} title={formatDateTime(key.created_at)}>
                {timeAgo(key.created_at)}
              </td>
              <td class={[table.td, 'text-subtle']}>
                {key.last_used_at ? timeAgo(key.last_used_at) : 'Nunca'}
              </td>
              <td class={[table.td, 'text-right']}>
                {#if key.revoked_at}
                  <Badge tone="danger">Revocada {timeAgo(key.revoked_at)}</Badge>
                {:else}
                  <Button size="xs" variant="secondary-destructive" onclick={() => askRevoke(key)}>
                    Revocar
                  </Button>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="p-4 text-sm text-subtle" role="status">Cargando…</p>
    {/if}
  </LayerCard>

  <LayerCard title="Cómo se usan">
    <p class="text-sm text-default">
      Envía la clave en la cabecera <code class="font-mono text-xs"
        >Authorization: Bearer taut_live_…</code
      >. La organización va en la propia clave. Una clave de lectura solo consulta; una de escritura
      también gestiona monitores. Ninguna toca miembros, canales, otras API keys ni la organización.
    </p>
  </LayerCard>
{/if}

<Dialog
  bind:open={creating}
  title={created ? 'Copia tu API key' : 'Nueva API key'}
  description={created
    ? 'Es la única vez que se muestra completa. Guárdala en tu gestor de secretos.'
    : 'Te pediremos la contraseña si hace más de 5 minutos que entraste.'}
>
  {#if created}
    <div class="space-y-4 text-sm">
      {#if copyFailed}
        <Banner tone="warning">
          El navegador no dejó copiar. Haz clic en el texto para seleccionarlo y cópialo a mano.
        </Banner>
      {/if}
      <div>
        <p class={label}>API key «{created.name}»</p>
        <code
          class="mt-1.5 block rounded-md bg-recessed px-3 py-2 font-mono text-xs break-all text-strong ring ring-hairline select-all"
          >{created.key}</code
        >
      </div>
      <div>
        <p class={label}>Pruébala</p>
        <code
          class="mt-1.5 block rounded-md bg-recessed px-3 py-2 font-mono text-xs break-all text-default ring ring-hairline select-all"
          >{curlExample(API_URL, created.key)}</code
        >
      </div>
    </div>
  {:else}
    <form id={formId} class="space-y-4" onsubmit={create} novalidate>
      {#if createError}<Banner tone="danger">{createError}</Banner>{/if}
      <div>
        <label class={label} for="{formId}-name">Nombre</label>
        <input
          id="{formId}-name"
          class={input}
          bind:value={values.name}
          placeholder="CI de GitHub"
          aria-invalid={errors.name ? true : undefined}
        />
        {#if errors.name}
          <p class={fieldError}>{errors.name}</p>
        {:else}
          <p class={help}>Dónde se usa, para reconocerla al revocarla.</p>
        {/if}
      </div>
      <div>
        <label class={label} for="{formId}-scope">Permiso</label>
        <Select id="{formId}-scope" bind:value={values.scope}>
          {#each scopes as scope (scope)}
            <option value={scope}>{SCOPE_LABELS[scope]}</option>
          {/each}
        </Select>
        {#if errors.scope}
          <p class={fieldError}>{errors.scope}</p>
        {:else}
          <p class={help}>{SCOPE_HINTS[values.scope]}</p>
        {/if}
      </div>
    </form>
  {/if}
  {#snippet actions()}
    {#if created}
      <Button variant="ghost" onclick={() => copy('curl')}>
        {copied === 'curl' ? 'Copiado' : 'Copiar curl'}
      </Button>
      <Button onclick={() => copy('key')}>{copied === 'key' ? 'Copiada' : 'Copiar clave'}</Button>
      <Button variant="primary" onclick={() => (creating = false)}>Ya la guardé</Button>
    {:else}
      <Button variant="ghost" onclick={() => (creating = false)} disabled={saving}>Cancelar</Button>
      <Button type="submit" form={formId} variant="primary" disabled={saving}>
        {saving ? 'Creando…' : 'Crear API key'}
      </Button>
    {/if}
  {/snippet}
</Dialog>

<Dialog
  bind:open={confirmingRevoke}
  alert
  title="¿Revocar la API key «{revoking?.name ?? ''}»?"
  description="Todo lo que la use recibirá 401 desde ya. Esto no se puede deshacer."
>
  {#snippet actions()}
    <Button variant="ghost" onclick={() => (confirmingRevoke = false)}>Cancelar</Button>
    <Button variant="destructive" onclick={() => revoking && revoke(revoking)}>
      Revocar API key
    </Button>
  {/snippet}
</Dialog>
