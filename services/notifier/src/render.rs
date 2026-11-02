//! What an alert says, per channel kind. Pure; user-facing text is in Spanish.

use std::fmt::Write;

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use taut_model::events::{AccountEmail, IncidentChanged};

/// An alert rendered for every channel kind.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    /// Email subject.
    pub subject: String,
    /// Email body, plain text.
    pub text: String,
    /// Email body, HTML.
    pub html: String,
    /// Discord incoming webhook body.
    pub discord: Value,
    /// Slack incoming webhook body.
    pub slack: Value,
    /// Generic webhook body.
    pub webhook: Value,
}

/// Renders an incident transition. `panel_url` is the base URL of the panel.
pub fn incident(event: &IncidentChanged, panel_url: &str) -> Rendered {
    let link = format!(
        "{}/monitors/{}",
        panel_url.trim_end_matches('/'),
        event.monitor_id
    );
    match event.resolved_at {
        None => incident_opened(event, &link),
        Some(resolved_at) => incident_resolved(event, resolved_at, &link),
    }
}

fn incident_opened(event: &IncidentChanged, link: &str) -> Rendered {
    let name = &event.monitor_name;
    let started = format_time(event.started_at);
    let preheader = format!(
        "El monitor «{name}» está caído desde el {started}. Causa: {}.",
        event.cause
    );
    let body = format!(
        r#"{banner}
<h1 style="margin: 0 0 12px 0; font-size: 20px; font-weight: 600; color: #09090b; letter-spacing: -0.3px;">
  El monitor «{name_escaped}» no responde
</h1>
<p style="margin: 0 0 24px 0; font-size: 14px; line-height: 1.6; color: #52525b;">
  El chequeo programado ha detectado un fallo en el servicio. A continuación se presentan los detalles del incidente detectado:
</p>
{details}
{button}
<p style="margin: 0; font-size: 12px; line-height: 1.5; color: #a1a1aa;">
  Recibirás otra notificación cuando el servicio se recupere satisfactoriamente.
</p>"#,
        banner = status_banner("#dc2626", "#fef2f2", "#fecaca", "#b91c1c", "Monitor Caído"),
        name_escaped = escape_html(name),
        details = detail_table(&[
            ("Monitor", name, false),
            ("Detectado", &started, false),
            ("Causa", &event.cause, true),
        ]),
        button = primary_button(link, "Ver monitor en Taut →"),
    );
    Rendered {
        subject: format!("[Taut] Caído: {name}"),
        text: format!(
            "El monitor «{name}» está caído desde el {started}.\n\n\
             Causa: {cause}\n\n\
             Detalle: {link}\n",
            cause = event.cause
        ),
        html: layout(&body, &preheader),
        discord: json!({
            "content": format!("🔴 **{name}** está caído: {}\n{link}", event.cause)
        }),
        slack: json!({
            "text": format!("🔴 *{name}* está caído: {}\n<{link}|Ver en Taut>", event.cause)
        }),
        webhook: webhook_body("incident.opened", event, link),
    }
}

fn incident_resolved(event: &IncidentChanged, resolved_at: DateTime<Utc>, link: &str) -> Rendered {
    let name = &event.monitor_name;
    let duration = format_duration((resolved_at - event.started_at).num_seconds());
    let resolved = format_time(resolved_at);
    let preheader = format!("El monitor «{name}» se recuperó el {resolved} tras {duration} caído.");
    let body = format!(
        r#"{banner}
<h1 style="margin: 0 0 12px 0; font-size: 20px; font-weight: 600; color: #09090b; letter-spacing: -0.3px;">
  El monitor «{name_escaped}» está operativo
</h1>
<p style="margin: 0 0 24px 0; font-size: 14px; line-height: 1.6; color: #52525b;">
  El chequeo de salud ha vuelto a responder con éxito. El incidente ha sido resuelto automáticamente.
</p>
{details}
{button}"#,
        banner = status_banner(
            "#16a34a",
            "#f0fdf4",
            "#bbf7d0",
            "#15803d",
            "Servicio Recuperado"
        ),
        name_escaped = escape_html(name),
        details = detail_table(&[
            ("Monitor", name, false),
            ("Tiempo caído", &duration, false),
            ("Recuperado", &resolved, false),
            ("Causa original", &event.cause, true),
        ]),
        button = primary_button(link, "Ver historial del incidente →"),
    );
    Rendered {
        subject: format!("[Taut] Recuperado: {name}"),
        text: format!(
            "El monitor «{name}» se recuperó el {resolved} después de {duration} caído.\n\n\
             Causa original: {cause}\n\n\
             Detalle: {link}\n",
            cause = event.cause
        ),
        html: layout(&body, &preheader),
        discord: json!({
            "content": format!("🟢 **{name}** se recuperó tras {duration}.\n{link}")
        }),
        slack: json!({
            "text": format!("🟢 *{name}* se recuperó tras {duration}.\n<{link}|Ver en Taut>")
        }),
        webhook: webhook_body("incident.resolved", event, link),
    }
}

/// Renders the message of the "send test" button.
pub fn test_message(channel_name: &str) -> Rendered {
    let text = format!(
        "Mensaje de prueba del canal «{channel_name}» de Taut. Si lo ves, las alertas llegarán aquí."
    );
    let preheader = format!("Mensaje de prueba del canal «{channel_name}» de Taut.");
    let body = format!(
        r#"{banner}
<h1 style="margin: 0 0 12px 0; font-size: 20px; font-weight: 600; color: #09090b; letter-spacing: -0.3px;">
  Canal de alertas configurado con éxito
</h1>
<p style="margin: 0 0 20px 0; font-size: 14px; line-height: 1.6; color: #52525b;">
  Este es un mensaje de prueba emitido para el canal <strong>«{channel_name_escaped}»</strong>.
</p>
<table role="presentation" border="0" cellpadding="0" cellspacing="0" width="100%" style="background-color: #fcfcfc; border: 1px solid #e4e4e7; border-left: 3px solid #09090b; border-radius: 0 8px 8px 0; margin-bottom: 28px;">
  <tr>
    <td style="padding: 16px; font-size: 14px; line-height: 1.5; color: #27272a;">
      Si estás leyendo este mensaje, las notificaciones críticas de caídas y recuperaciones de tus monitores llegarán puntualmente a esta bandeja.
    </td>
  </tr>
</table>
<p style="margin: 0; font-size: 13px; line-height: 1.5; color: #71717a;">
  No se requiere ninguna acción adicional por tu parte.
</p>"#,
        banner = status_banner(
            "#18181b",
            "#f4f4f5",
            "#e4e4e7",
            "#18181b",
            "Prueba de Canal"
        ),
        channel_name_escaped = escape_html(channel_name),
    );
    Rendered {
        subject: "[Taut] Prueba de alertas".to_owned(),
        text: format!("{text}\n"),
        html: layout(&body, &preheader),
        discord: json!({ "content": format!("🔔 {text}") }),
        slack: json!({ "text": format!("🔔 {text}") }),
        webhook: json!({ "event": "test", "channel": channel_name, "message": text }),
    }
}

/// Renders an account email (verification, password reset, invitation). Email only: the
/// webhook fields are empty, these never go to a channel.
pub fn account_email(email: &AccountEmail) -> Rendered {
    let (subject, text, html) = match email {
        AccountEmail::VerificationRequested {
            name,
            link,
            expires_at,
            ..
        } => verification_email(name, link, *expires_at),
        AccountEmail::PasswordResetRequested {
            name,
            link,
            expires_at,
            ..
        } => password_reset_email(name, link, *expires_at),
        AccountEmail::AlreadyRegistered {
            name,
            login_link,
            reset_link,
            ..
        } => already_registered_email(name, login_link, reset_link),
        AccountEmail::MemberInvited {
            tenant_name,
            invited_by,
            role,
            link,
            expires_at,
            ..
        } => member_invited_email(tenant_name, invited_by, role, link, *expires_at),
    };
    Rendered {
        subject,
        text,
        html,
        discord: Value::Null,
        slack: Value::Null,
        webhook: Value::Null,
    }
}

fn verification_email(
    name: &str,
    link: &str,
    expires_at: DateTime<Utc>,
) -> (String, String, String) {
    let expires = format_time(expires_at);
    let subject = "[Taut] Confirma tu correo".to_owned();
    let text = format!(
        "Hola, {name}.\n\n\
         Confirma tu dirección de correo para entrar a taut:\n\n{link}\n\n\
         El enlace vale hasta el {expires}. Si no creaste una cuenta, ignora este mensaje.\n"
    );
    let preheader = "Confirma tu dirección de correo electrónico para acceder a Taut.";
    let body = format!(
        r#"<h1 style="margin: 0 0 12px 0; font-size: 20px; font-weight: 600; color: #09090b; letter-spacing: -0.3px;">
  Confirma tu correo electrónico
</h1>
<p style="margin: 0 0 16px 0; font-size: 14px; line-height: 1.6; color: #52525b;">
  Hola, <strong>{name_escaped}</strong>.
</p>
<p style="margin: 0 0 28px 0; font-size: 14px; line-height: 1.6; color: #52525b;">
  Gracias por registrarte en Taut. Para activar tu cuenta y configurar tus primeros monitores, por favor confirma tu dirección pulsando el siguiente botón:
</p>
{button}
{expiry}
{raw_link}"#,
        name_escaped = escape_html(name),
        button = primary_button(link, "Confirmar mi correo →"),
        expiry = expiry_callout(&format!(
            "El enlace es válido hasta el <strong>{expires}</strong>.<br>Si tú no creaste una cuenta en Taut, puedes ignorar este mensaje sin problema."
        )),
        raw_link = raw_link_section(
            "¿El botón no responde? Copia y pega esta dirección en tu navegador:",
            link
        ),
    );
    (subject, text, layout(&body, preheader))
}

fn password_reset_email(
    name: &str,
    link: &str,
    expires_at: DateTime<Utc>,
) -> (String, String, String) {
    let expires = format_time(expires_at);
    let subject = "[Taut] Restablece tu contraseña".to_owned();
    let text = format!(
        "Hola, {name}.\n\n\
         Alguien pidió restablecer la contraseña de tu cuenta de Taut. Si fuiste tú, \
         elige una nueva aquí:\n\n{link}\n\n\
         El enlace vale hasta el {expires}. Si no lo pediste, tu contraseña sigue igual.\n"
    );
    let preheader = "Recibimos una solicitud para restablecer tu contraseña en Taut.";
    let body = format!(
        r#"<h1 style="margin: 0 0 12px 0; font-size: 20px; font-weight: 600; color: #09090b; letter-spacing: -0.3px;">
  Restablece tu contraseña
</h1>
<p style="margin: 0 0 16px 0; font-size: 14px; line-height: 1.6; color: #52525b;">
  Hola, <strong>{name_escaped}</strong>.
</p>
<p style="margin: 0 0 28px 0; font-size: 14px; line-height: 1.6; color: #52525b;">
  Hemos recibido una solicitud para cambiar la contraseña de tu cuenta de Taut. Si fuiste tú, haz clic en el botón de abajo para definir una nueva:
</p>
{button}
{expiry}
{raw_link}"#,
        name_escaped = escape_html(name),
        button = primary_button(link, "Elegir nueva contraseña →"),
        expiry = expiry_callout(&format!(
            "El enlace expira el <strong>{expires}</strong>.<br>Si no solicitaste restablecer tu clave, puedes ignorar este mensaje; tu cuenta sigue completamente protegida."
        )),
        raw_link = raw_link_section("Enlace alternativo directo:", link),
    );
    (subject, text, layout(&body, preheader))
}

fn already_registered_email(
    name: &str,
    login_link: &str,
    reset_link: &str,
) -> (String, String, String) {
    let subject = "[Taut] Ya tienes una cuenta".to_owned();
    let text = format!(
        "Hola, {name}.\n\n\
         Alguien intentó registrarse en Taut con tu dirección, pero ya tienes una cuenta.\n\n\
         Entra aquí: {login_link}\n\
         ¿Olvidaste la contraseña? {reset_link}\n\n\
         Si no fuiste tú, no hace falta que hagas nada.\n"
    );
    let preheader = "Alguien intentó registrar una cuenta con tu dirección en Taut.";
    let body = format!(
        r#"<h1 style="margin: 0 0 12px 0; font-size: 20px; font-weight: 600; color: #09090b; letter-spacing: -0.3px;">
  Ya estás registrado en Taut
</h1>
<p style="margin: 0 0 16px 0; font-size: 14px; line-height: 1.6; color: #52525b;">
  Hola, <strong>{name_escaped}</strong>.
</p>
<p style="margin: 0 0 24px 0; font-size: 14px; line-height: 1.6; color: #52525b;">
  Recientemente alguien intentó registrarse en Taut utilizando esta dirección de correo, pero ya posees una cuenta activa con nosotros.
</p>
<table role="presentation" border="0" cellpadding="0" cellspacing="0" style="margin-bottom: 16px;">
  <tr>
    <td style="border-radius: 8px; background-color: #09090b;">
      <a href="{login_link}" target="_blank" style="display: inline-block; padding: 10px 22px; font-size: 14px; font-weight: 500; color: #fafafa; text-decoration: none; border-radius: 8px;">
        Iniciar sesión
      </a>
    </td>
  </tr>
</table>
<p style="margin: 0 0 24px 0; font-size: 13px; color: #71717a;">
  ¿Olvidaste tu contraseña? <a href="{reset_link}" target="_blank" style="color: #18181b; font-weight: 500; text-decoration: underline;">Restablécela aquí</a>
</p>
<p style="margin: 0; font-size: 13px; line-height: 1.5; color: #71717a;">
  Si no fuiste tú quien intentó registrarse, puedes desestimar este mensaje de forma segura. Tus monitores y datos permanecen intactos.
</p>"#,
        name_escaped = escape_html(name),
    );
    (subject, text, layout(&body, preheader))
}

fn member_invited_email(
    tenant_name: &str,
    invited_by: &str,
    role: &str,
    link: &str,
    expires_at: DateTime<Utc>,
) -> (String, String, String) {
    let expires = format_time(expires_at);
    let role_text = role_label(role);
    let subject = format!("[Taut] Te invitaron a {tenant_name}");
    let text = format!(
        "{invited_by} te invitó a «{tenant_name}» en Taut como {role_text}.\n\n\
         Acepta la invitación aquí:\n\n{link}\n\n\
         La invitación vale hasta el {expires}.\n"
    );
    let preheader = format!(
        "{invited_by} te invitó a unirte al equipo de {tenant_name} en Taut como {role_text}."
    );
    let body = format!(
        r#"{banner}
<h1 style="margin: 0 0 12px 0; font-size: 20px; font-weight: 600; color: #09090b; letter-spacing: -0.3px;">
  Únete a «{tenant_escaped}» en Taut
</h1>
<p style="margin: 0 0 24px 0; font-size: 14px; line-height: 1.6; color: #52525b;">
  <strong>{invited_by_escaped}</strong> te ha invitado a colaborar en el equipo de <strong>{tenant_escaped}</strong> en Taut con el rol de <strong>{role_text}</strong>.
</p>
{details}
{button}
{expiry}
{raw_link}"#,
        banner = status_banner(
            "#09090b",
            "#f4f4f5",
            "#e4e4e7",
            "#09090b",
            "Invitación de Organización"
        ),
        tenant_escaped = escape_html(tenant_name),
        invited_by_escaped = escape_html(invited_by),
        details = detail_table(&[
            ("Organización", tenant_name, false),
            ("Invitado por", invited_by, false),
            ("Rol asignado", role_text, false),
        ]),
        button = primary_button(link, "Aceptar invitación →"),
        expiry = expiry_callout(&format!(
            "Esta invitación es válida hasta el <strong>{expires}</strong>."
        )),
        raw_link = raw_link_section("Enlace directo para unirse:", link),
    );
    (subject, text, layout(&body, &preheader))
}

fn role_label(role: &str) -> &str {
    match role {
        "owner" => "propietario",
        _ => "miembro",
    }
}

fn webhook_body(event_name: &str, event: &IncidentChanged, link: &str) -> Value {
    json!({
        "event": event_name,
        "incident": {
            "id": event.incident_id,
            "started_at": event.started_at,
            "resolved_at": event.resolved_at,
            "cause": event.cause,
            "duration_s": event.resolved_at.map(|resolved| (resolved - event.started_at).num_seconds()),
        },
        "monitor": { "id": event.monitor_id, "name": event.monitor_name },
        "tenant_id": event.tenant_id,
        "url": link,
    })
}

/// `16/10/2026 04:18 UTC`.
fn format_time(time: DateTime<Utc>) -> String {
    time.format("%d/%m/%Y %H:%M UTC").to_string()
}

/// `45 s`, `3 min 20 s`, `2 h 5 min`.
pub fn format_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    match seconds {
        0..60 => format!("{seconds} s"),
        60..3600 => {
            let rest = seconds % 60;
            if rest == 0 {
                format!("{} min", seconds / 60)
            } else {
                format!("{} min {rest} s", seconds / 60)
            }
        }
        _ => {
            let minutes = (seconds % 3600) / 60;
            if minutes == 0 {
                format!("{} h", seconds / 3600)
            } else {
                format!("{} h {minutes} min", seconds / 3600)
            }
        }
    }
}

/// HTML escaping for safe injection into templates.
fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Full HTML email layout container following Taut UI design tokens.
fn layout(content_html: &str, preheader_text: &str) -> String {
    let preheader = if preheader_text.is_empty() {
        String::new()
    } else {
        format!(
            r#"<div style="display: none; max-height: 0px; overflow: hidden; font-size: 1px; line-height: 1px; color: #fafafa;">{}</div>"#,
            escape_html(preheader_text)
        )
    };

    format!(
        r##"<!DOCTYPE html>
<html lang="es" xmlns="http://www.w3.org/1999/xhtml">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="X-UA-Compatible" content="IE=edge">
  <meta name="x-apple-disable-message-reformatting">
  <title>Taut</title>
</head>
<body style="margin: 0; padding: 0; width: 100% !important; -webkit-text-size-adjust: 100%; -ms-text-size-adjust: 100%; background-color: #fafafa; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; color: #18181b;">
  {preheader}
  <table role="presentation" border="0" cellpadding="0" cellspacing="0" width="100%" style="background-color: #fafafa; table-layout: fixed;">
    <tr>
      <td align="center" style="padding: 40px 16px;">
        <table role="presentation" border="0" cellpadding="0" cellspacing="0" width="100%" style="max-width: 560px; background-color: #ffffff; border-radius: 12px; border: 1px solid #e4e4e7; overflow: hidden; box-shadow: 0 1px 3px rgba(0,0,0,0.04);">
          <tr>
            <td style="padding: 32px 32px 24px 32px; border-bottom: 1px solid #f4f4f5;">
              <table role="presentation" border="0" cellpadding="0" cellspacing="0" width="100%">
                <tr>
                  <td>
                    <a href="https://taut.lat" target="_blank" style="text-decoration: none; display: inline-flex; align-items: center; gap: 8px;">
                      <svg width="24" height="24" viewBox="50 33 162 155" fill="#09090b" style="display: inline-block; vertical-align: middle;">
                        <polygon points="68,95 50,64 68,33 176,33 194,64 86,64"/>
                        <polygon points="68,95 104,95 68,157 50,126"/>
                        <polygon points="194,64 212,95 194,126 158,126 122,188 86,188 68,157 104,157 140,95 176,95"/>
                      </svg>
                      <span style="font-size: 18px; font-weight: 700; color: #09090b; letter-spacing: -0.4px; vertical-align: middle; margin-left: 6px;">taut</span>
                    </a>
                  </td>
                </tr>
              </table>
            </td>
          </tr>
          <tr>
            <td style="padding: 32px 32px 36px 32px;">
              {content_html}
            </td>
          </tr>
          <tr>
            <td style="padding: 24px 32px; background-color: #fafafa; border-top: 1px solid #e4e4e7;">
              <table role="presentation" border="0" cellpadding="0" cellspacing="0" width="100%">
                <tr>
                  <td style="font-size: 12px; line-height: 1.5; color: #71717a; text-align: left;">
                    Este correo se envió automáticamente desde <a href="https://taut.lat" style="color: #18181b; font-weight: 500; text-decoration: underline;">Taut</a>.<br>
                    Monitoreo continuo de disponibilidad y notificaciones en tiempo real.
                  </td>
                </tr>
              </table>
            </td>
          </tr>
        </table>
      </td>
    </tr>
  </table>
</body>
</html>"##
    )
}

fn status_banner(
    dot_color: &str,
    bg_color: &str,
    border_color: &str,
    text_color: &str,
    label: &str,
) -> String {
    format!(
        r#"<table role="presentation" border="0" cellpadding="0" cellspacing="0" width="100%" style="background-color: {bg_color}; border: 1px solid {border_color}; border-radius: 8px; margin-bottom: 24px;">
  <tr>
    <td style="padding: 12px 16px;">
      <table role="presentation" border="0" cellpadding="0" cellspacing="0">
        <tr>
          <td style="vertical-align: middle; padding-right: 8px;">
            <span style="display: inline-block; width: 10px; height: 10px; border-radius: 50%; background-color: {dot_color};"></span>
          </td>
          <td style="font-size: 13px; font-weight: 600; color: {text_color}; text-transform: uppercase; letter-spacing: 0.04em;">
            {label}
          </td>
        </tr>
      </table>
    </td>
  </tr>
</table>"#
    )
}

fn primary_button(link: &str, label: &str) -> String {
    format!(
        r#"<table role="presentation" border="0" cellpadding="0" cellspacing="0" style="margin-bottom: 28px;">
  <tr>
    <td style="border-radius: 8px; background-color: #09090b;">
      <a href="{link}" target="_blank" style="display: inline-block; padding: 10px 22px; font-size: 14px; font-weight: 500; color: #fafafa; text-decoration: none; border-radius: 8px;">
        {label}
      </a>
    </td>
  </tr>
</table>"#
    )
}

fn detail_table(rows: &[(&str, &str, bool)]) -> String {
    let mut rows_html = String::new();
    let total = rows.len();
    for (i, (label, value, is_mono)) in rows.iter().enumerate() {
        let is_last = i + 1 == total;
        let border_style = if is_last {
            ""
        } else {
            "border-bottom: 1px solid #f4f4f5;"
        };
        let val_style = if *is_mono {
            format!(
                "padding: 12px 16px; {border_style} font-size: 13px; font-family: ui-monospace, Menlo, Consolas, monospace; color: #b91c1c;"
            )
        } else {
            format!(
                "padding: 12px 16px; {border_style} font-size: 13px; font-weight: 500; color: #18181b;"
            )
        };
        let label_esc = escape_html(label);
        let val_esc = escape_html(value);
        let _ = write!(
            rows_html,
            r#"<tr>
  <td style="padding: 12px 16px; {border_style} font-size: 13px; color: #71717a; width: 120px;">
    {label_esc}
  </td>
  <td style="{val_style}">
    {val_esc}
  </td>
</tr>"#
        );
    }

    format!(
        r#"<table role="presentation" border="0" cellpadding="0" cellspacing="0" width="100%" style="background-color: #fcfcfc; border: 1px solid #e4e4e7; border-radius: 8px; margin-bottom: 28px; border-collapse: separate;">
{rows_html}
</table>"#
    )
}

fn expiry_callout(content_html: &str) -> String {
    format!(
        r#"<table role="presentation" border="0" cellpadding="0" cellspacing="0" width="100%" style="background-color: #fafafa; border: 1px solid #e4e4e7; border-radius: 8px; margin-bottom: 24px;">
  <tr>
    <td style="padding: 14px 16px; font-size: 12px; line-height: 1.6; color: #71717a;">
      {content_html}
    </td>
  </tr>
</table>"#
    )
}

fn raw_link_section(title: &str, link: &str) -> String {
    format!(
        r#"<p style="margin: 0 0 8px 0; font-size: 12px; color: #a1a1aa;">
  {title}
</p>
<p style="margin: 0; font-size: 12px; font-family: ui-monospace, Menlo, Consolas, monospace; color: #52525b; word-break: break-all;">
  {link}
</p>"#
    )
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn event(resolved_after_s: Option<i64>) -> IncidentChanged {
        let started_at = DateTime::from_timestamp(1_792_124_280, 0).unwrap();
        IncidentChanged {
            trace_id: "trace".to_owned(),
            incident_id: Uuid::nil(),
            tenant_id: Uuid::nil(),
            monitor_id: Uuid::nil(),
            monitor_name: "API".to_owned(),
            started_at,
            resolved_at: resolved_after_s.map(|s| started_at + chrono::Duration::seconds(s)),
            cause: "status 503 (expected 2xx)".to_owned(),
        }
    }

    #[test]
    fn opened_alert_names_monitor_cause_and_link() {
        let rendered = incident(&event(None), "http://panel.test/");
        assert_eq!(rendered.subject, "[Taut] Caído: API");
        assert!(rendered.text.contains("Causa: status 503 (expected 2xx)"));
        assert!(
            rendered
                .text
                .contains("http://panel.test/monitors/00000000-0000-0000-0000-000000000000")
        );
        assert!(
            rendered
                .html
                .contains("http://panel.test/monitors/00000000-0000-0000-0000-000000000000")
        );
        assert!(rendered.html.contains("Monitor Caído"));
        assert!(rendered.html.contains("status 503 (expected 2xx)"));
        assert!(rendered.html.contains("https://taut.lat"));
        assert!(
            rendered.discord["content"]
                .as_str()
                .unwrap()
                .starts_with("🔴 **API**")
        );
        assert_eq!(rendered.webhook["event"], "incident.opened");
        assert_eq!(rendered.webhook["incident"]["duration_s"], Value::Null);
    }

    #[test]
    fn resolved_alert_says_how_long_it_was_down() {
        let rendered = incident(&event(Some(200)), "http://panel.test");
        assert_eq!(rendered.subject, "[Taut] Recuperado: API");
        assert!(rendered.text.contains("después de 3 min 20 s caído"));
        assert!(rendered.html.contains("3 min 20 s"));
        assert!(rendered.html.contains("Servicio Recuperado"));
        assert!(rendered.html.contains("Ver historial del incidente →"));
        assert!(
            rendered.slack["text"]
                .as_str()
                .unwrap()
                .contains("tras 3 min 20 s")
        );
        assert_eq!(rendered.webhook["event"], "incident.resolved");
        assert_eq!(rendered.webhook["incident"]["duration_s"], 200);
    }

    #[test]
    fn test_alert_renders_correctly() {
        let rendered = test_message("Ops Team");
        assert_eq!(rendered.subject, "[Taut] Prueba de alertas");
        assert!(rendered.text.contains("Ops Team"));
        assert!(rendered.html.contains("Ops Team"));
        assert!(rendered.html.contains("Prueba de Canal"));
        assert!(
            rendered
                .html
                .contains("Canal de alertas configurado con éxito")
        );
    }

    #[test]
    fn account_emails_carry_the_link_and_the_deadline() {
        let expires_at = DateTime::from_timestamp(1_792_210_680, 0).unwrap();
        let verification = account_email(&AccountEmail::VerificationRequested {
            trace_id: "trace".to_owned(),
            to: "ana@example.com".to_owned(),
            name: "Ana".to_owned(),
            link: "http://panel.test/verify-email?token=abc".to_owned(),
            expires_at,
        });
        assert_eq!(verification.subject, "[Taut] Confirma tu correo");
        assert!(verification.text.contains("Hola, Ana."));
        assert!(
            verification
                .text
                .contains("http://panel.test/verify-email?token=abc")
        );
        assert!(verification.text.contains("17/10/2026 04:18 UTC"));
        assert!(verification.html.contains("Confirmar mi correo →"));
        assert!(
            verification
                .html
                .contains("http://panel.test/verify-email?token=abc")
        );

        let reset = account_email(&AccountEmail::PasswordResetRequested {
            trace_id: "trace".to_owned(),
            to: "ana@example.com".to_owned(),
            name: "Ana".to_owned(),
            link: "http://panel.test/reset-password?token=xyz".to_owned(),
            expires_at,
        });
        assert_eq!(reset.subject, "[Taut] Restablece tu contraseña");
        assert!(reset.text.contains("token=xyz"));
        assert!(reset.html.contains("Elegir nueva contraseña →"));
        assert!(reset.html.contains("token=xyz"));

        let notice = account_email(&AccountEmail::AlreadyRegistered {
            trace_id: "trace".to_owned(),
            to: "ana@example.com".to_owned(),
            name: "Ana".to_owned(),
            login_link: "http://panel.test/login".to_owned(),
            reset_link: "http://panel.test/forgot-password".to_owned(),
        });
        assert_eq!(notice.subject, "[Taut] Ya tienes una cuenta");
        assert!(notice.text.contains("http://panel.test/forgot-password"));
        assert!(notice.html.contains("http://panel.test/login"));
        assert!(notice.html.contains("http://panel.test/forgot-password"));
        assert!(notice.html.contains("Iniciar sesión"));

        let invite = account_email(&AccountEmail::MemberInvited {
            trace_id: "trace".to_owned(),
            to: "bea@example.com".to_owned(),
            tenant_id: Uuid::nil(),
            tenant_name: "Acme".to_owned(),
            invited_by: "Ana".to_owned(),
            role: "owner".to_owned(),
            link: "http://panel.test/invite/abc".to_owned(),
            expires_at,
        });
        assert_eq!(invite.subject, "[Taut] Te invitaron a Acme");
        assert!(invite.text.contains("como propietario"));
        assert!(invite.html.contains("rol de <strong>propietario</strong>"));
        assert!(invite.html.contains("Aceptar invitación →"));
        assert!(invite.html.contains("http://panel.test/invite/abc"));
    }

    #[test]
    fn durations_use_the_largest_units() {
        assert_eq!(format_duration(45), "45 s");
        assert_eq!(format_duration(120), "2 min");
        assert_eq!(format_duration(7500), "2 h 5 min");
        assert_eq!(format_duration(-3), "0 s");
    }

    #[test]
    fn html_escaping_sanitizes_special_characters() {
        assert_eq!(
            escape_html(r#"<script>alert("hello & 'bye'")</script>"#),
            "&lt;script&gt;alert(&quot;hello &amp; &#39;bye&#39;&quot;)&lt;/script&gt;"
        );
    }
}
