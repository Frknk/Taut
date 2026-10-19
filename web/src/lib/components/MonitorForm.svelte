<script lang="ts">
  import { Collapsible, RadioGroup } from 'bits-ui';
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon';
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
  import { fieldError, focusRing, help as hint, input, label } from '#lib/ui.js';

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

  // The periods start folded; an error inside them unfolds them, or it would go unseen.
  let periodsOpen = $state(false);
  $effect(() => {
    if (errors.confirmation_period_s || errors.recovery_period_s) periodsOpen = true;
  });
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

  <div>
    <span id="type-label" class={label}>Tipo</span>
    <RadioGroup.Root
      bind:value={() => values.type, (type) => (values.type = type as MonitorKind)}
      disabled={editing}
      orientation="horizontal"
      aria-labelledby="type-label"
      aria-describedby={editing ? 'type-hint' : undefined}
      class="mt-1.5 grid grid-cols-2 gap-2 sm:grid-cols-4"
    >
      {#each kinds as kind (kind)}
        <RadioGroup.Item
          value={kind}
          class={[
            'press flex h-9 cursor-pointer items-center justify-center rounded-lg px-3 text-sm font-medium ring',
            'bg-control text-default ring-line hover:bg-tint',
            'data-[state=checked]:bg-brand data-[state=checked]:text-on-brand data-[state=checked]:ring-brand',
            'data-disabled:cursor-not-allowed data-[state=unchecked]:data-disabled:text-inactive data-[state=unchecked]:data-disabled:hover:bg-control',
            focusRing,
          ]}
        >
          {KIND_LABELS[kind]}
        </RadioGroup.Item>
      {/each}
    </RadioGroup.Root>
    {#if editing}
      <p id="type-hint" class={hint}>El tipo no se puede cambiar; crea otro monitor.</p>
    {/if}
  </div>

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
      Un heartbeat no tiene destino: tu tarea programada llama a una URL secreta de Taut. Si no
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
      <Select
        id="interval"
        bind:value={values.interval_s}
        items={INTERVALS.map((s) => ({ value: s, label: `Cada ${formatSeconds(s)}` }))}
      />
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
    <Collapsible.Root
      bind:open={periodsOpen}
      class="rounded-lg bg-base px-4 py-3 ring ring-hairline"
    >
      <Collapsible.Trigger
        class={[
          'group flex w-full cursor-pointer items-center gap-1.5 rounded-sm text-left font-medium text-default',
          focusRing,
        ]}
      >
        <CaretDownIcon
          size={14}
          class="-rotate-90 text-subtle group-data-[state=open]:rotate-0"
          aria-hidden="true"
        />
        Confirmación y recuperación
      </Collapsible.Trigger>
      <Collapsible.Content class="mt-4 grid gap-6 sm:grid-cols-2">
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
      </Collapsible.Content>
    </Collapsible.Root>
  {/if}

  <Button type="submit" variant="primary" disabled={submitting}>
    {submitting ? 'Guardando…' : submitLabel}
  </Button>
</form>
