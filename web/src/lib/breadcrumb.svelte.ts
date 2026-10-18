// The name a nested page gives to the breadcrumb, such as the monitor of `/monitors/{id}`.

/** Shared with the top bar of the panel layout. */
export const breadcrumb = $state<{ name: string | null }>({ name: null });

/**
 * Names the current page's resource in the breadcrumb while the page is mounted. Call it from
 * the page's script with a getter, so the name follows the data once it loads.
 */
export function useBreadcrumbName(name: () => string | null | undefined): void {
  $effect(() => {
    breadcrumb.name = name() ?? null;
    return () => {
      breadcrumb.name = null;
    };
  });
}
