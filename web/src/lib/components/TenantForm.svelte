<script lang="ts">
  import { describeError, request } from '#lib/api.js';
  import { slugify } from '#lib/tenant-form.js';
  import type { TenantWithRole } from '#lib/types.js';
  import { fieldError, input, label } from '#lib/ui.js';

  interface Props {
    /** Id of the `<form>`: the submit button lives outside it, with `form={id}`. */
    id: string;
    /** True while the request is in flight, to disable the submit button. */
    creating?: boolean;
    oncreated: (tenant: TenantWithRole) => void;
  }

  /** Name and slug of a new tenant; the slug follows the name until the user edits it. */
  let { id, creating = $bindable(false), oncreated }: Props = $props();

  let name = $state('');
  let slug = $state('');
  let slugTouched = $state(false);
  let error = $state('');

  $effect(() => {
    if (!slugTouched) slug = slugify(name);
  });

  async function create(event: SubmitEvent) {
    event.preventDefault();
    error = '';
    creating = true;
    try {
      oncreated(await request<TenantWithRole>('POST', '/v1/tenants', { body: { name, slug } }));
    } catch (err) {
      error = describeError(err);
    } finally {
      creating = false;
    }
  }
</script>

<form {id} class="space-y-4" onsubmit={create}>
  <div>
    <label class={label} for="{id}-name">Nombre</label>
    <input id="{id}-name" class={input} bind:value={name} required maxlength="100" />
  </div>
  <div>
    <label class={label} for="{id}-slug">Slug (dirección de la página de estado)</label>
    <input
      id="{id}-slug"
      class={[input, 'font-mono']}
      bind:value={slug}
      oninput={() => (slugTouched = true)}
      required
      minlength="3"
      maxlength="40"
      pattern="[a-z0-9][a-z0-9\-]*[a-z0-9]"
    />
  </div>
  {#if error}<p class={fieldError}>{error}</p>{/if}
</form>
