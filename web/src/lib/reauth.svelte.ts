// The step-up prompt. When the api or auth answers `403 reauth_required`, the API client asks
// here, the `ReauthDialog` mounted in the app layout opens, and the promise resolves once the
// user confirmed their password (the call is then retried) or gave up. Several calls refused
// at once share one prompt.

class ReauthPrompt {
  open = $state(false);
  private pending: Promise<boolean> | null = null;
  private settle: ((confirmed: boolean) => void) | null = null;

  /** Opens the dialog (if not open already) and waits for the user. */
  request(): Promise<boolean> {
    if (!this.pending) {
      this.pending = new Promise<boolean>((resolve) => {
        this.settle = resolve;
      });
      this.open = true;
    }
    return this.pending;
  }

  /** The dialog reports the outcome: `true` after a successful re-authentication. */
  resolve(confirmed: boolean): void {
    this.open = false;
    const settle = this.settle;
    this.settle = null;
    this.pending = null;
    settle?.(confirmed);
  }
}

export const reauthPrompt = new ReauthPrompt();
