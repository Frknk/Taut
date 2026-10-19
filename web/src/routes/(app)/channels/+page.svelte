<script lang="ts">
  import { onMount } from 'svelte';
  import { flip } from 'svelte/animate';

  import Badge from '#lib/components/Badge.svelte';
  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import CopyLabel from '#lib/components/CopyLabel.svelte';
  import Dialog from '#lib/components/Dialog.svelte';
  import Empty from '#lib/components/Empty.svelte';
  import LayerCard from '#lib/components/LayerCard.svelte';
  import Select from '#lib/components/Select.svelte';
  import Tooltip from '#lib/components/Tooltip.svelte';
  import { describeError, request } from '#lib/api.js';
  import {
    DESTINATION_HINTS,
    emptyChannel,
    toChannelInput,
    validateChannel,
    type ChannelErrors,
  } from '#lib/channel-form.js';
  import { CHANNEL_LABELS, timeAgo } from '#lib/format.js';
  import { FLIP, leave } from '#lib/motion.js';
  import { selectedTenant } from '#lib/tenant.svelte.js';
  import type { AlertChannel, ChannelKind, ChannelWithSecret, RotatedSecret } from '#lib/types.js';
  import { card, fieldError, help, input, label } from '#lib/ui.js';

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
  let rotating = $state<AlertChannel | null>(null);
  let confirmingRotate = $state(false);

  // A webhook's signing secret, shown this one time after creating or rotating it.
  let secret = $state<{ channel: string; value: string } | null>(null);
  let showingSecret = $state(false);
  let copied = $state(false);
  let copyFailed = $state(false);

  // Closing the dialog any other way (Escape) also forgets the secret.
  $effect(() => {
    if (!showingSecret) secret = null;
  });

  function showSecret(channel: string, value: string) {
    secret = { channel, value };
    copied = false;
    copyFailed = false;
    showingSecret = true;
  }

  async function copySecret() {
    if (!secret) return;
    try {
      await navigator.clipboard.writeText(secret.value);
      copied = true;
      copyFailed = false;
      setTimeout(() => (copied = false), 2000);
    } catch {
      // The browser refused the clipboard: the text stays selectable with one click.
      copied = false;
      copyFailed = true;
    }
  }

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
      const created = await request<ChannelWithSecret>('POST', '/v1/alert-channels', {
        tenant: tenantId,
        body: toChannelInput(values),
      });
      values = emptyChannel();
      if (created.signing_secret) showSecret(created.name, created.signing_secret);
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

  function askRotate(channel: AlertChannel) {
    rotating = channel;
    confirmingRotate = true;
  }

  async function rotate(channel: AlertChannel) {
    confirmingRotate = false;
    try {
      const rotated = await request<RotatedSecret>(
        'POST',
        `/v1/alert-channels/${channel.id}/rotate-secret`,
        { tenant: tenantId },
      );
      showSecret(channel.name, rotated.signing_secret);
      await load();
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
      <li
        class="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3"
        animate:flip={FLIP}
        out:leave
      >
        <Badge>{CHANNEL_LABELS[channel.type]}</Badge>
        {#if channel.type === 'webhook'}
          {#if channel.signing_secret_set}
            <Badge tone="success">Firmado</Badge>
          {:else}
            <Tooltip
              content="Se creó antes de que taut firmara los webhooks. Genera un secreto para firmarlo."
            >
              <Badge tone="warning">Sin firma</Badge>
            </Tooltip>
          {/if}
        {/if}
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
              <Tooltip content={channel.last_delivery_error} class="text-danger">
                Último envío falló {timeAgo(channel.last_delivery_at)}
              </Tooltip>
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
          {#if channel.type === 'webhook'}
            <Button size="sm" onclick={() => askRotate(channel)}>
              {channel.signing_secret_set ? 'Rotar secreto' : 'Generar secreto'}
            </Button>
          {/if}
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

<Dialog
  bind:open={confirmingRotate}
  alert
  title={rotating?.signing_secret_set
    ? `¿Rotar el secreto de «${rotating?.name ?? ''}»?`
    : `¿Generar un secreto para «${rotating?.name ?? ''}»?`}
  description={rotating?.signing_secret_set
    ? 'El secreto actual deja de valer desde ya: actualiza tu receptor con el nuevo antes del próximo incidente.'
    : 'Desde ya, cada envío llevará la cabecera X-Taut-Signature firmada con el secreto nuevo.'}
>
  {#snippet actions()}
    <Button variant="ghost" onclick={() => (confirmingRotate = false)}>Cancelar</Button>
    <Button variant="primary" onclick={() => rotating && rotate(rotating)}>
      {rotating?.signing_secret_set ? 'Rotar secreto' : 'Generar secreto'}
    </Button>
  {/snippet}
</Dialog>

<Dialog
  bind:open={showingSecret}
  title="Copia el secreto de firma"
  description="Es la única vez que se muestra. Guárdalo en tu receptor para verificar cada envío."
>
  {#if secret}
    <div class="space-y-4 text-sm">
      {#if copyFailed}
        <Banner tone="warning">
          El navegador no dejó copiar. Haz clic en el texto para seleccionarlo y cópialo a mano.
        </Banner>
      {/if}
      <div>
        <p class={label}>Webhook «{secret.channel}»</p>
        <code
          class="mt-1.5 block rounded-md bg-recessed px-3 py-2 font-mono text-xs break-all text-strong ring ring-hairline select-all"
          >{secret.value}</code
        >
      </div>
      <p class="text-subtle">
        Cada envío lleva <code class="text-[0.9em]">X-Taut-Signature: t=…,v1=…</code>, donde
        <code class="text-[0.9em]">v1</code> es el HMAC-SHA256 en hexadecimal de
        <code class="text-[0.9em]">t + "." + cuerpo</code> con este secreto. Rechaza las firmas con más
        de 5 minutos.
      </p>
    </div>
  {/if}
  {#snippet actions()}
    <Button onclick={copySecret}
      ><CopyLabel {copied} label="Copiar secreto" done="Copiado" /></Button
    >
    <Button variant="primary" onclick={() => (showingSecret = false)}>Ya lo guardé</Button>
  {/snippet}
</Dialog>

{#if loaded}
  <form class="mt-8" onsubmit={create} novalidate>
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
          {#if errors.destination}
            <p class={fieldError}>{errors.destination}</p>
          {:else if values.type === 'webhook'}
            <p class={help}>
              Firmamos cada envío con HMAC-SHA256; al crearlo verás el secreto para verificarlo.
            </p>
          {/if}
        </div>
        <Button type="submit" variant="primary" disabled={saving}>
          {saving ? 'Guardando…' : 'Agregar canal'}
        </Button>
      </div>
    </LayerCard>
  </form>
{/if}
