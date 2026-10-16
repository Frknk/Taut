// Validation of the account forms, in Spanish. The rules mirror services/auth: the service
// stays the source of truth, these only save a round trip.

export const PASSWORD_MIN = 8;
export const PASSWORD_MAX = 128;
export const NAME_MAX = 100;

export interface LoginValues {
  email: string;
  password: string;
}

export interface RegisterValues {
  name: string;
  email: string;
  password: string;
  confirm: string;
}

export interface NewPasswordValues {
  password: string;
  confirm: string;
}

export interface PasswordChangeValues extends NewPasswordValues {
  current: string;
}

export type AuthErrors<T> = Partial<Record<keyof T, string>>;

/** Loose shape check, like the service: something@something.something. */
export function looksLikeEmail(value: string): boolean {
  const email = value.trim();
  const at = email.indexOf('@');
  if (at < 1 || at !== email.lastIndexOf('@')) return false;
  const domain = email.slice(at + 1);
  return (
    domain.includes('.') && !domain.startsWith('.') && !domain.endsWith('.') && !/\s/.test(email)
  );
}

function passwordError(password: string, email = ''): string | undefined {
  const length = [...password].length;
  if (length < PASSWORD_MIN) return `Mínimo ${PASSWORD_MIN} caracteres.`;
  if (length > PASSWORD_MAX) return `Máximo ${PASSWORD_MAX} caracteres.`;
  if (email && password.trim().toLowerCase() === email.trim().toLowerCase()) {
    return 'No puede ser tu correo.';
  }
  return undefined;
}

export function validateLogin(values: LoginValues): AuthErrors<LoginValues> {
  const errors: AuthErrors<LoginValues> = {};
  if (!looksLikeEmail(values.email)) errors.email = 'Escribe un correo válido.';
  if (!values.password) errors.password = 'Escribe tu contraseña.';
  return errors;
}

export function validateRegister(values: RegisterValues): AuthErrors<RegisterValues> {
  const errors: AuthErrors<RegisterValues> = {};
  const name = values.name.trim();
  if (!name) errors.name = 'Escribe tu nombre.';
  else if ([...name].length > NAME_MAX) errors.name = `Máximo ${NAME_MAX} caracteres.`;
  if (!looksLikeEmail(values.email)) errors.email = 'Escribe un correo válido.';
  const password = passwordError(values.password, values.email);
  if (password) errors.password = password;
  else if (values.confirm !== values.password) errors.confirm = 'Las contraseñas no coinciden.';
  return errors;
}

export function validateNewPassword(
  values: NewPasswordValues,
  email = '',
): AuthErrors<NewPasswordValues> {
  const errors: AuthErrors<NewPasswordValues> = {};
  const password = passwordError(values.password, email);
  if (password) errors.password = password;
  else if (values.confirm !== values.password) errors.confirm = 'Las contraseñas no coinciden.';
  return errors;
}

export function validatePasswordChange(
  values: PasswordChangeValues,
  email = '',
): AuthErrors<PasswordChangeValues> {
  const errors: AuthErrors<PasswordChangeValues> = validateNewPassword(values, email);
  if (!values.current) errors.current = 'Escribe tu contraseña actual.';
  else if (values.current === values.password && !errors.password) {
    errors.password = 'La nueva contraseña es igual a la actual.';
  }
  return errors;
}
