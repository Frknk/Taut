<script lang="ts">
  import { untrack } from 'svelte';

  import Banner from '#lib/components/Banner.svelte';
  import Button from '#lib/components/Button.svelte';
  import Select from '#lib/components/Select.svelte';
  import { KIND_LABELS, formatSeconds } from '#lib/format.js';
  import {
    INTERVALS,
    toInput,
    validateForm,
    type FormErrors,
    type MonitorFormValues,
  } from '#lib/monitor-form.js';
  import type { MonitorInput, MonitorKind } from '#lib/types.js';
  import { fieldError, help as hint, input, label } from '#lib/ui.js';

  interface Props {
    initial: MonitorFormValues;
    /** The type cannot change once the monitor exists. */
    editing?: boolean;
    submitLabel: string;
    onsubmit: (input: MonitorInput) => Promise<void>;
  }

  let { initial, editing = false, submitLabel, onsubmit }: Props = $props();

  let values = $state(untrack(() => ({ ...initial })));
  let errors = $state<FormErrors>({});
  let attempted = $state(false);
  let submitting = $state(false);

  // After the first attempt, errors follow the fields as the user fixes them.
  $effect(() => {
    if (attempted) errors = validateForm(values);
  });

  const kinds = Object.keys(KIND_LABELS) as MonitorKind[];
  const isHttp = $derived(values.type === 'http' || values.type === 'keyword');

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    attempted = true;
    errors = validateForm(values);
    if (Object.keys(errors).length > 0) return;
    submitting = true;
    try {
      await onsubmit(toInput(values));
    } finally {
      submitting = false;
    }
  }
</script>

<form class="space-y-6" onsubmit={submit} novalidate>
  <div>
    <label class={label} for="name">Nombre</label>
    <input
      id="name"
      class={input}
      bind:value={values.name}
      placeholder="Sitio principal"
      aria-invalid={errors.name ? 'true' : undefined}
    />
    {#if errors.name}<p class={fieldError}>{errors.name}</p>{/if}
  </div>

  <fieldset>
    <legend class={label}>Tipo</legend>
    <div class="mt-1.5 grid grid-cols-2 gap-2 sm:grid-cols-4">
      {#each kinds as kind (kind)}
        <label
          class={[
            'flex h-9 cursor-pointer items-center justify-center rounded-lg px-3 text-sm font-medium ring',
            'has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-focus',
            values.type === kind
              ? 'bg-brand text-on-brand ring-brand'
              : 'bg-control text-default ring-line hover:bg-tint',
            editing && values.type !== kind && 'cursor-not-allowed text-inactive hover:bg-control',
          ]}
        >
          <input
            type="radio"
            class="sr-only"
            name="type"
            value={kind}
            bind:group={values.type}
            disabled={editing}
          />
          {KIND_LABELS[kind]}
        </label>
      {/each}
    </div>
    {#if editing}<p class={hint}>El tipo no se puede cambiar; crea otro monitor.</p>{/if}
  </fieldset>

  {#if values.type !== 'heartbeat'}
    <div>
      <label class={label} for="target">{isHttp ? 'URL' : 'Host y puerto'}</label>
      <input
        id="target"
        class={[input, 'font-mono']}
        aria-invalid={errors.target ? 'true' : undefined}
        bind:value={values.target}
        placeholder={isHttp ? 'https://ejemplo.com/health' : 'db.ejemplo.com:5432'}
      />
      {#if errors.target}<p class={fieldError}>{errors.target}</p>{/if}
    </div>
  {:else}
    <Banner>
      Un heartbeat no tiene destino: tu tarea programada llama a una URL secreta de taut. Si no
      llama a tiempo, se abre un incidente.
    </Banner>
  {/if}

  {#if values.type === 'keyword'}
    <div>
      <label class={label} for="keyword">Texto esperado</label>
      <input id="keyword" class={input} bind:value={values.keyword} placeholder="Bienvenido" />
      <p class={hint}>El chequeo falla si la respuesta no contiene este texto.</p>
      {#if errors.keyword}<p class={fieldError}>{errors.keyword}</p>{/if}
    </div>
  {/if}

  <div class="grid gap-6 sm:grid-cols-2">
    <div>
      <label class={label} for="interval">
        {values.type === 'heartbeat' ? 'Frecuencia esperada del ping' : 'Frecuencia'}
      </label>
      <Select id="interval" bind:value={values.interval_s}>
        {#each INTERVALS as interval (interval)}
          <option value={interval}>Cada {formatSeconds(interval)}</option>
        {/each}
      </Select>
      {#if errors.interval_s}<p class={fieldError}>{errors.interval_s}</p>{/if}
    </div>

    {#if values.type === 'heartbeat'}
      <div>
        <label class={label} for="grace">Periodo de gracia (s)</label>
        <input id="grace" type="number" min="0" class={input} bind:value={values.grace_period_s} />
        <p class={hint}>Silencio extra tolerado después de la frecuencia.</p>
        {#if errors.grace_period_s}<p class={fieldError}>{errors.grace_period_s}</p>{/if}
      </div>
    {:else}
      <div>
        <label class={label} for="timeout">Timeout (s)</label>
        <input id="timeout" type="number" min="1" class={input} bind:value={values.timeout_s} />
        {#if errors.timeout_s}<p class={fieldError}>{errors.timeout_s}</p>{/if}
      </div>
    {/if}
  </div>

  {#if isHttp}
    <div>
      <label class={label} for="expected">Código HTTP esperado</label>
      <input
        id="expected"
        inputmode="numeric"
        class={input}
        bind:value={values.expected_status}
        placeholder="Cualquier 2xx"
      />
      {#if errors.expected_status}<p class={fieldError}>{errors.expected_status}</p>{/if}
    </div>
  {/if}

  {#if values.type !== 'heartbeat'}
    <details class="rounded-lg bg-base px-4 py-3 ring ring-hairline">
      <summary class="cursor-pointer rounded-sm font-medium text-default">
        Confirmación y recuperación
      </summary>
      <div class="mt-4 grid gap-6 sm:grid-cols-2">
        <div>
          <label class={label} for="confirmation">Periodo de confirmación (s)</label>
          <input
            id="confirmation"
            type="number"
            min="0"
            class={input}
            bind:value={values.confirmation_period_s}
          />
          <p class={hint}>Cuánto debe durar la falla antes de abrir un incidente.</p>
          {#if errors.confirmation_period_s}
            <p class={fieldError}>{errors.confirmation_period_s}</p>
          {/if}
        </div>
        <div>
          <label class={label} for="recovery">Periodo de recuperación (s)</label>
          <input
            id="recovery"
            type="number"
            min="0"
            class={input}
            bind:value={values.recovery_period_s}
          />
          <p class={hint}>Cuánto debe estar sano antes de cerrar el incidente.</p>
          {#if errors.recovery_period_s}<p class={fieldError}>{errors.recovery_period_s}</p>{/if}
        </div>
      </div>
    </details>
  {/if}

  <Button type="submit" variant="primary" disabled={submitting}>
    {submitting ? 'Guardando…' : submitLabel}
  </Button>
</form>
