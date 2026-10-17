<script lang="ts">
  import { onMount } from 'svelte';

  import Badge from '#lib/components/Badge.svelte';
  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import Dialog from '#lib/components/Dialog.svelte';
  import Empty from '#lib/components/Empty.svelte';
  import LayerCard from '#lib/components/LayerCard.svelte';
  import Select from '#lib/components/Select.svelte';
  import { describeError, request } from '#lib/api.js';
  import {
    DESTINATION_HINTS,
    emptyChannel,
    toChannelInput,
    validateChannel,
    type ChannelErrors,
  } from '#lib/channel-form.js';
  import { CHANNEL_LABELS, timeAgo } from '#lib/format.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { AlertChannel, ChannelKind } from '#lib/types.js';
  import { card, fieldError, input, label } from '#lib/ui.js';

  const tenantId = $derived(selectedTenant.current?.id ?? '');
  const kinds = Object.keys(CHANNEL_LABELS) as ChannelKind[];

  let channels = $state<AlertChannel[]>([]);
  let loaded = $state(false);
  let error = $state('');

  let values = $state(emptyChannel());
  let errors = $state<ChannelErrors>({});
  let saving = $state(false);
  let notice = $state('');
  let deleting = $state<AlertChannel | null>(null);
  let confirmingDelete = $state(false);

  async function load() {
    try {
      channels = await request<AlertChannel[]>('GET', '/v1/alert-channels', { tenant: tenantId });
      error = '';
    } catch (err) {
      error = describeError(err);
    } finally {
      loaded = true;
    }
  }

  async function create(event: SubmitEvent) {
    event.preventDefault();
    errors = validateChannel(values);
    if (Object.keys(errors).length > 0) return;
    saving = true;
    try {
      await request<AlertChannel>('POST', '/v1/alert-channels', {
        tenant: tenantId,
        body: toChannelInput(values),
      });
      values = emptyChannel();
      await load();
    } catch (err) {
      error = describeError(err);
    } finally {
      saving = false;
    }
  }

  async function toggle(channel: AlertChannel) {
    try {
      await request<AlertChannel>('PUT', `/v1/alert-channels/${channel.id}`, {
        tenant: tenantId,
        body: {
          name: channel.name,
          type: channel.type,
          config: channel.config,
          active: !channel.active,
        },
      });
      await load();
    } catch (err) {
      error = describeError(err);
    }
  }

  async function test(channel: AlertChannel) {
    try {
      await request('POST', `/v1/alert-channels/${channel.id}/test`, { tenant: tenantId });
      notice = `Prueba enviada a «${channel.name}». El resultado aparece en unos segundos.`;
      setTimeout(load, 3000);
    } catch (err) {
      error = describeError(err);
    }
  }

  function askRemove(channel: AlertChannel) {
    deleting = channel;
    confirmingDelete = true;
  }

  async function remove(channel: AlertChannel) {
    confirmingDelete = false;
    try {
      await request<void>('DELETE', `/v1/alert-channels/${channel.id}`, { tenant: tenantId });
      await load();
    } catch (err) {
      error = describeError(err);
    }
  }

  onMount(load);
</script>

<svelte:head>
  <title>Canales · Taut</title>
</svelte:head>

<h1 class="text-2xl font-semibold text-strong">Canales de alerta</h1>
<p class="mt-0.5 text-[13px]/[18px] text-subtle">
  Cada incidente avisa a todos los canales activos al abrirse y al cerrarse.
</p>

{#if error}
  <Banner tone="danger" class="mt-6">{error}</Banner>
{/if}
{#if notice}
  <Banner tone="success" class="mt-6">{notice}</Banner>
{/if}

{#if loaded && channels.length === 0}
  <div class="mt-6">
    <Empty
      title="Todavía no hay canales"
      text="Sin canales, los incidentes se abren pero nadie se entera. Agrega uno abajo."
    />
  </div>
{:else if channels.length > 0}
  <ul class={[card, 'mt-6 divide-y divide-hairline']}>
    {#each channels as channel (channel.id)}
      <li class="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3">
        <Badge>{CHANNEL_LABELS[channel.type]}</Badge>
        <div class="min-w-0 flex-1">
          <p class={['font-medium', channel.active ? 'text-default' : 'text-subtle']}>
            {channel.name}{channel.active ? '' : ' (silenciado)'}
          </p>
          <p class="truncate text-xs text-subtle">
            <code>{channel.config.to ?? channel.config.url}</code>
          </p>
        </div>
        <span class="text-xs">
          {#if channel.last_delivery_at}
            {#if channel.last_delivery_error}
              <span class="text-danger" title={channel.last_delivery_error}>
                Último envío falló {timeAgo(channel.last_delivery_at)}
              </span>
            {:else}
              <span class="text-success">Último envío OK {timeAgo(channel.last_delivery_at)}</span>
            {/if}
          {:else}
            <span class="text-subtle">Sin envíos</span>
          {/if}
        </span>
        <div class="flex gap-2">
          <Button size="sm" onclick={() => test(channel)}>Probar</Button>
          <Button size="sm" onclick={() => toggle(channel)}>
            {channel.active ? 'Silenciar' : 'Activar'}
          </Button>
          <Button size="sm" variant="secondary-destructive" onclick={() => askRemove(channel)}>
            Eliminar
          </Button>
        </div>
      </li>
    {/each}
  </ul>
{/if}

<Dialog
  bind:open={confirmingDelete}
  alert
  title="¿Eliminar el canal «{deleting?.name ?? ''}»?"
  description="Los próximos incidentes ya no avisarán a este canal. Esto no se puede deshacer."
>
  {#snippet actions()}
    <Button variant="ghost" onclick={() => (confirmingDelete = false)}>Cancelar</Button>
    <Button variant="destructive" onclick={() => deleting && remove(deleting)}>
      Eliminar canal
    </Button>
  {/snippet}
</Dialog>

<form class="mt-8 max-w-2xl" onsubmit={create} novalidate>
  <LayerCard title="Nuevo canal">
    <div class="space-y-4">
      <div class="grid gap-4 sm:grid-cols-2">
        <div>
          <label class={label} for="channel-type">Tipo</label>
          <Select
            id="channel-type"
            bind:value={values.type}
            items={kinds.map((kind) => ({ value: kind, label: CHANNEL_LABELS[kind] }))}
          />
        </div>
        <div>
          <label class={label} for="channel-name">Nombre</label>
          <input
            id="channel-name"
            class={input}
            bind:value={values.name}
            placeholder="Guardia"
            aria-invalid={errors.name ? 'true' : undefined}
          />
          {#if errors.name}<p class={fieldError}>{errors.name}</p>{/if}
        </div>
      </div>
      <div>
        <label class={label} for="channel-destination">
          {values.type === 'email' ? 'Correo' : 'URL del webhook'}
        </label>
        <input
          id="channel-destination"
          class={[input, 'font-mono']}
          bind:value={values.destination}
          placeholder={DESTINATION_HINTS[values.type]}
          aria-invalid={errors.destination ? 'true' : undefined}
        />
        {#if errors.destination}<p class={fieldError}>{errors.destination}</p>{/if}
      </div>
      <Button type="submit" variant="primary" disabled={saving}>
        {saving ? 'Guardando…' : 'Agregar canal'}
      </Button>
    </div>
  </LayerCard>
</form>
