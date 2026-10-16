<script lang="ts">
  import { onMount } from 'svelte';

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

  const tenantId = $derived(selectedTenant.current?.id ?? '');
  const kinds = Object.keys(CHANNEL_LABELS) as ChannelKind[];

  let channels = $state<AlertChannel[]>([]);
  let loaded = $state(false);
  let error = $state('');

  let values = $state(emptyChannel());
  let errors = $state<ChannelErrors>({});
  let saving = $state(false);
  let notice = $state('');

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

  async function remove(channel: AlertChannel) {
    if (!confirm(`¿Eliminar el canal «${channel.name}»?`)) return;
    try {
      await request<void>('DELETE', `/v1/alert-channels/${channel.id}`, { tenant: tenantId });
      await load();
    } catch (err) {
      error = describeError(err);
    }
  }

  onMount(load);

  const input =
    'mt-1 block w-full rounded-lg border border-neutral-300 bg-white px-3 py-2 text-sm focus:border-neutral-500 focus:outline-none';
  const button =
    'rounded-lg border border-neutral-300 bg-white px-3 py-1 text-xs text-neutral-700 hover:bg-neutral-50';
</script>

<svelte:head>
  <title>Canales · taut</title>
</svelte:head>

<h1 class="text-xl font-semibold text-neutral-900">Canales de alerta</h1>
<p class="mt-1 text-sm text-neutral-500">
  Cada incidente avisa a todos los canales activos al abrirse y al cerrarse.
</p>

{#if error}
  <p class="mt-6 rounded-lg bg-red-50 px-4 py-3 text-sm text-red-700">{error}</p>
{/if}
{#if notice}
  <p class="mt-6 rounded-lg bg-emerald-50 px-4 py-3 text-sm text-emerald-700">{notice}</p>
{/if}

{#if loaded && channels.length === 0}
  <p
    class="mt-6 rounded-xl border border-dashed border-neutral-300 p-6 text-center text-sm text-neutral-600"
  >
    Sin canales: los incidentes se abren pero nadie se entera. Agrega uno abajo.
  </p>
{:else if channels.length > 0}
  <ul class="mt-6 divide-y divide-neutral-100 rounded-xl border border-neutral-200 bg-white">
    {#each channels as channel (channel.id)}
      <li class="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3 text-sm">
        <span class="rounded-full bg-neutral-100 px-2.5 py-0.5 text-xs text-neutral-700">
          {CHANNEL_LABELS[channel.type]}
        </span>
        <div class="min-w-0 flex-1">
          <p class={['font-medium', channel.active ? 'text-neutral-900' : 'text-neutral-400']}>
            {channel.name}{channel.active ? '' : ' (silenciado)'}
          </p>
          <p class="truncate text-xs text-neutral-500">
            {channel.config.to ?? channel.config.url}
          </p>
        </div>
        <span class="text-xs">
          {#if channel.last_delivery_at}
            {#if channel.last_delivery_error}
              <span class="text-red-700" title={channel.last_delivery_error}>
                Último envío falló {timeAgo(channel.last_delivery_at)}
              </span>
            {:else}
              <span class="text-emerald-700"
                >Último envío OK {timeAgo(channel.last_delivery_at)}</span
              >
            {/if}
          {:else}
            <span class="text-neutral-400">Sin envíos</span>
          {/if}
        </span>
        <div class="flex gap-2">
          <button class={button} onclick={() => test(channel)}>Probar</button>
          <button class={button} onclick={() => toggle(channel)}>
            {channel.active ? 'Silenciar' : 'Activar'}
          </button>
          <button class={[button, 'text-red-700']} onclick={() => remove(channel)}>Eliminar</button>
        </div>
      </li>
    {/each}
  </ul>
{/if}

<form
  class="mt-8 max-w-2xl space-y-4 rounded-xl border border-neutral-200 bg-white p-6"
  onsubmit={create}
  novalidate
>
  <h2 class="text-sm font-medium text-neutral-900">Nuevo canal</h2>
  <div class="grid gap-4 sm:grid-cols-2">
    <div>
      <label class="text-xs text-neutral-500" for="channel-type">Tipo</label>
      <select id="channel-type" class={input} bind:value={values.type}>
        {#each kinds as kind (kind)}
          <option value={kind}>{CHANNEL_LABELS[kind]}</option>
        {/each}
      </select>
    </div>
    <div>
      <label class="text-xs text-neutral-500" for="channel-name">Nombre</label>
      <input id="channel-name" class={input} bind:value={values.name} placeholder="Guardia" />
      {#if errors.name}<p class="mt-1 text-xs text-red-600">{errors.name}</p>{/if}
    </div>
  </div>
  <div>
    <label class="text-xs text-neutral-500" for="channel-destination">
      {values.type === 'email' ? 'Correo' : 'URL del webhook'}
    </label>
    <input
      id="channel-destination"
      class={input}
      bind:value={values.destination}
      placeholder={DESTINATION_HINTS[values.type]}
    />
    {#if errors.destination}<p class="mt-1 text-xs text-red-600">{errors.destination}</p>{/if}
  </div>
  <button
    class="rounded-lg bg-neutral-900 px-4 py-2 text-sm font-medium text-white hover:bg-neutral-700 disabled:opacity-50"
    disabled={saving}
  >
    {saving ? 'Guardando…' : 'Agregar canal'}
  </button>
</form>
